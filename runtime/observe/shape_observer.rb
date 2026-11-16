# Loaded with `ruby -r` ahead of an emitted CRuby tree's boot when the Spinel build needs what a gem does to
# a model's shape as it loads. It records each definition, mixin and class macro a gem's code runs on a
# model, and writes them as source patches: the statements to write after the model's own statement that
# set them off, and the gem statements that did them, which the Spinel tree then no longer runs.
require "json"
require "prism"

module RoundhouseShapeObserver
  OUT = ENV.fetch("RH_SHAPE_OUT")
  # Not as given: a temporary directory's path reaches Ruby through a symlink (`/var` on macOS), and frames name the real one.
  GEMS = ENV.fetch("RH_SHAPE_GEMS", "").split(File::PATH_SEPARATOR).reject(&:empty?).map { |g| File.realpath(g) }
  APP = File.realpath(ENV.fetch("RH_SHAPE_APP"))
  @effects = []
  @refusals = []
  @parsed = {}
  @captures = []
  @sites = {}

  class << self
    attr_reader :effects, :refusals

    def gem_file?(path)
      GEMS.any? { |g| path.start_with?(g) }
    end

    # The patch's entry for a gem statement: its file as `<gem dir>/<path in the gem>`, its range and its text.
    def gem_statement(path, stmt)
      root = GEMS.find { |g| path.start_with?(g) }
      return nil unless root && stmt
      stmt = widened(stmt, path)
      s, e = stmt.location.start_offset, stmt.location.end_offset
      { "gem_file" => File.basename(root) + path[root.size..], "range" => [s, e], "text" => File.binread(path).byteslice(s, e - s).force_encoding(Encoding::UTF_8) }
    end

    # The app class's own class-body statement whose run set this off: its file in the tree and its first line.
    def trigger(owner)
      path, = Object.const_source_location(owner.name)
      # Not the innermost frame there: a DSL block's own lines run inside the class-body statement that hands it over.
      body = "<class:#{owner.name.split("::").last}>"
      caller_locations.reverse_each do |l|
        next unless l.path == path && l.base_label == body
        text = File.readlines(path)[l.lineno - 1]
        return { "file" => path.delete_prefix(APP + "/"), "text" => text.strip } if text
      end
      nil
    end

    # The gem statement that made the call itself; not one further out, whose callee (the framework's
    # `after_initialize`) may define on classes of its own.
    def gem_site
      l = caller_locations.find { |c| c.path != __FILE__ && !c.path.start_with?("<internal") }
      l && gem_file?(l.path) ? [l.path, l.lineno] : nil
    end

    def parse(path)
      @parsed[path] ||= Prism.parse_file(path)
    end

    PURE = %i[const_defined? method_defined? private_method_defined? respond_to? instance_methods].freeze

    # The statement, or the conditional around it when that holds it alone, has no `else` and only asks the class (`unless klass.const_defined?(c)`).
    def widened(stmt, path)
      parent = enclosing_conditional(parse(path).value, stmt)
      return stmt unless parent
      only = parent.statements&.body&.size == 1
      bare = parent.respond_to?(:subsequent) ? parent.subsequent.nil? : parent.else_clause.nil?
      pure = parent.predicate.is_a?(Prism::CallNode) && PURE.include?(parent.predicate.name)
      only && bare && pure ? parent : stmt
    end

    def enclosing_conditional(node, stmt)
      node.compact_child_nodes.each do |child|
        if (child.is_a?(Prism::IfNode) || child.is_a?(Prism::UnlessNode)) && child.statements&.body&.any? { |s| s.equal?(stmt) || s.location.start_offset == stmt.location.start_offset }
          return child
        end
        found = enclosing_conditional(child, stmt)
        return found if found
      end
      nil
    end

    # The outermost statement (a direct child of a statement list) that starts on `line`.
    def statement_at(path, line)
      walk = lambda do |node|
        node.compact_child_nodes.each do |child|
          if child.is_a?(Prism::StatementsNode)
            child.body.each do |stmt|
              loc = stmt.location
              return stmt if loc.start_line == line
              found = walk.call(stmt) if loc.start_line < line && line <= loc.end_line
              return found if found
            end
          else
            found = walk.call(child)
            return found if found
          end
        end
        nil
      end
      walk.call(parse(path).value)
    end

    # A class the app's own source declares, whose shape the Spinel tree writes where the class is.
    def app_class?(owner)
      return false unless owner.is_a?(Module) && owner.name
      path, = Object.const_source_location(owner.name)
      path && path.start_with?(File.join(APP, "app"))
    rescue NameError
      false
    end

    def literal(value)
      ok = [Symbol, String, Integer, Float, NilClass, TrueClass, FalseClass].any? { |k| value.is_a?(k) } ||
           (value.is_a?(Array) && value.all? { |v| literal(v) }) ||
           (value.is_a?(Hash) && value.all? { |k, v| literal(k) && literal(v) })
      ok ? value.inspect : nil
    end

    def refuse(why)
      @refusals << why
      nil
    end

    # `def name(params)` with the lambda's body, each local it captured read as the value it held.
    def definition(name, proc_)
      parts = body_of(name, proc_) or return nil
      "def #{name}(#{parts[0]})\n#{parts[1]}\nend"
    end

    # The same as a lambda literal, for a macro handed one.
    def lambda_literal(name, proc_)
      parts = body_of(name, proc_) or return nil
      "->(#{parts[0]}) { #{parts[1]} }"
    end

    MUTATORS = %i[<< push append unshift prepend concat insert []= store update merge! replace clear delete delete_if keep_if select! reject! map! collect! compact! flatten! uniq! sort! sort_by! shift pop fill transform_values! transform_keys!].freeze
    HARD_SCOPES = [Prism::DefNode, Prism::ClassNode, Prism::ModuleNode, Prism::SingletonClassNode].freeze
    # Not as written in a `def`: there `yield` and these read the method's own call, not the one the block was made in.
    ENCLOSING = [Prism::YieldNode, Prism::ForwardingSuperNode, Prism::SuperNode].freeze

    def body_of(name, proc_)
      path, line = proc_.source_location
      return refuse("#{name}: no source") unless path && File.file?(path)
      lambdas = []
      finder = lambda do |node|
        lambdas << node if (node.is_a?(Prism::LambdaNode) || node.is_a?(Prism::BlockNode)) && node.location.start_line == line
        node.compact_child_nodes.each { |c| finder.call(c) }
      end
      finder.call(parse(path).value)
      return refuse("#{name}: #{lambdas.size} blocks at #{path}:#{line}") unless lambdas.size == 1
      node = lambdas.first
      shape = node.parameters
      return refuse("#{name}: numbered or `it` parameters") if shape.is_a?(Prism::NumberedParametersNode) || shape.is_a?(Prism::ItParametersNode)
      params = shape&.parameters
      if params && (params.optionals.any? || params.keywords.any? { |k| k.is_a?(Prism::OptionalKeywordParameterNode) })
        return refuse("#{name}: a parameter's default would read where the block was made")
      end
      binding_ = proc_.binding
      edits = []
      walk = lambda do |n, level|
        case n
        when *HARD_SCOPES then return refuse("#{name}: a `#{n.type}` in the block")
        when *ENCLOSING then return refuse("#{name}: `#{n.slice[0, 20]}` reads the call the block was made in")
        when Prism::ImplicitNode then return refuse("#{name}: a shorthand `{ #{n.slice} }` reads a captured local")
        when Prism::CallNode
          # Not a captured Array or Hash the body changes: each call of the `def` would change a fresh copy.
          if MUTATORS.include?(n.name) && n.receiver.is_a?(Prism::LocalVariableReadNode) && n.receiver.depth > level &&
             [Array, Hash, String].any? { |k| binding_.local_variable_get(n.receiver.name).is_a?(k) }
            return refuse("#{name}: changes captured `#{n.receiver.name}` with `#{n.name}`")
          end
        when Prism::LocalVariableReadNode
          if n.depth > level
            value = binding_.local_variable_get(n.name)
            text = literal(value) or return refuse("#{name}: captured `#{n.name}` is not a literal (#{value.class})")
            @captures << [binding_, n.name, text, name]
            edits << [n.location.start_offset, n.location.end_offset, text]
          end
        when Prism::LocalVariableWriteNode, Prism::LocalVariableOperatorWriteNode, Prism::LocalVariableOrWriteNode, Prism::LocalVariableAndWriteNode, Prism::LocalVariableTargetNode
          return refuse("#{name}: writes captured `#{n.name}`") if n.depth > level
        when Prism::ConstantReadNode
          # Not left to the class it is written into: a constant is looked up where the block was written.
          value = (binding_.eval(n.name.to_s) rescue (return refuse("#{name}: `#{n.name}` names nothing where the block was made")))
          # Not the value it held as the app booted: read by its full name, a later assignment is what the method sees.
          nesting = binding_.eval("::Module.nesting")
          holder = (nesting + (nesting.first&.ancestors || []) + [Object]).find { |m| m.const_defined?(n.name, false) }
          text = value.is_a?(Module) && value.name ? "::#{value.name}" : holder && (holder == Object ? "::#{n.name}" : holder.name && "::#{holder.name}::#{n.name}")
          # Not a name that reads another value from the top level, or none (a `private_constant`).
          same = text && (TOPLEVEL_BINDING.eval(text).equal?(value) rescue false)
          return refuse("#{name}: `#{n.name}` cannot be named from the class it is written into") unless same
          edits << [n.location.start_offset, n.location.end_offset, text]
          return true
        end
        inner = n.is_a?(Prism::BlockNode) || n.is_a?(Prism::LambdaNode) ? level + 1 : level
        n.compact_child_nodes.each { |c| return nil unless walk.call(c, inner) }
        true
      end
      body = node.body
      return nil if body && !walk.call(body, 0)
      source = File.binread(path)
      text = body ? source.byteslice(body.location.start_offset, body.location.end_offset - body.location.start_offset) : "nil"
      base = body ? body.location.start_offset : 0
      edits.sort_by { |s, _, _| -s }.each { |s, e, t| text = text.byteslice(0, s - base) + t + text.byteslice(e - base..) }
      signature = params ? source.byteslice(params.location.start_offset, params.location.end_offset - params.location.start_offset) : ""
      [signature, text]
    end

    # Each gem statement that changed a class's shape, and whether every class it changed is the app's.
    def note_site(site, owner)
      return unless site
      (@sites[site.join(":")] ||= []) << !!app_class?(owner)
    end

    def record(owner, event, statement, site = gem_site)
      return false unless site && app_class?(owner)
      trig = trigger(owner)
      return refuse("#{owner}: #{event[0, 60]} with no class-body statement of the app's behind it") unless trig
      stmt = statement_at(*site)
      entry = gem_statement(site[0], stmt)
      return refuse("#{owner}: no statement starts at #{site.join(":")}") unless entry
      @effects << entry.merge("owner" => owner.name, "trigger" => trig, "statement" => statement, "site" => site.join(":"))
      true
    end

    def write
      # Not what a capture held when the block was made, if the boot changed it after: the method reads it as it is.
      @captures.each do |b, local, text, name|
        refuse("#{name}: captured `#{local}` changed after the block was made") unless literal(b.local_variable_get(local)) == text
      end
      # Not a gem statement that also changed a class of the gem's or the framework's: read as `nil`, it would no longer change that one.
      @effects.each do |e|
        refuse("#{e["site"]} changes a class that is not the app's too") if e["site"] && @sites.fetch(e["site"], []).include?(false)
      end
      File.write(OUT, JSON.pretty_generate({ "effects" => @effects, "refusals" => @refusals.uniq }))
    end
  end

  module Recorder
    def define_method(name, body = nil, &block)
      proc_ = body || block
      site = RoundhouseShapeObserver.gem_site
      RoundhouseShapeObserver.note_site(site, self)
      if site && RoundhouseShapeObserver.app_class?(self)
        text = proc_.is_a?(Proc) ? RoundhouseShapeObserver.definition(name, proc_) : RoundhouseShapeObserver.refuse("#{self}##{name}: body is a #{proc_.class}")
        recorded = text && RoundhouseShapeObserver.record(self, text, text, site)
        if recorded && proc_.is_a?(Proc)
          path, line = proc_.source_location
          stmt = path && RoundhouseShapeObserver.gem_file?(path) && RoundhouseShapeObserver.statement_at(path, line)
          entry = stmt && RoundhouseShapeObserver.gem_statement(path, stmt)
          RoundhouseShapeObserver.effects << entry.merge("owner" => self.name, "site" => site.join(":")) if entry
        end
      end
      body ? super(name, body) : super(name, &block)
    end

    %i[include extend].each do |kind|
      define_method(kind) do |*mods|
        site = RoundhouseShapeObserver.gem_site
        RoundhouseShapeObserver.note_site(site, self)
        # Last first, as Ruby mixes them in: `include A, B` gives A the higher place.
        mods.reverse_each { |m| RoundhouseShapeObserver.record(self, "#{kind} ::#{m.name}", "#{kind} ::#{m.name}", site) if m.name }
        super(*mods)
      end
    end

    def ruby2_keywords(*names)
      site = RoundhouseShapeObserver.gem_site
      RoundhouseShapeObserver.note_site(site, self)
      names.each { |n| RoundhouseShapeObserver.record(self, "ruby2_keywords :#{n}", "ruby2_keywords :#{n}", site) }
      super
    end

    def const_set(name, value)
      site = RoundhouseShapeObserver.gem_site
      RoundhouseShapeObserver.note_site(site, self)
      if site && RoundhouseShapeObserver.app_class?(self)
        text = RoundhouseShapeObserver.literal(value)
        text ? RoundhouseShapeObserver.record(self, "#{name} = #{text}", "#{name} = #{text}", site) : RoundhouseShapeObserver.refuse("#{self}::#{name}: not a literal")
      end
      super
    end
  end
end

Module.prepend(RoundhouseShapeObserver::Recorder)

# A class macro a gem's code calls on a model (`base.after_initialize :aasm_ensure_initial_state`).
RoundhouseShapeObserver::MACRO = /\A(?:(?:after|before|around)_\w+|validates?|validates_\w+|scope|attribute|enum|serialize|store_accessor|has_many|has_one|belongs_to|has_and_belongs_to_many|delegate|alias_attribute)\z/
TracePoint.new(:call) do |tp|
  next unless tp.self.is_a?(Class) && RoundhouseShapeObserver::MACRO.match?(tp.method_id.to_s)
  next unless defined?(ActiveRecord::Base) && tp.self < ActiveRecord::Base
  caller_frame = caller_locations(2, 1).first
  next unless caller_frame && RoundhouseShapeObserver.gem_file?(caller_frame.path)
  next if RoundhouseShapeObserver.gem_file?(tp.path)
  RoundhouseShapeObserver.note_site([caller_frame.path, caller_frame.lineno], tp.self)
  next unless RoundhouseShapeObserver.app_class?(tp.self)
  args = []
  ok = tp.parameters.all? do |(kind, pname)|
    # Not skipped: an anonymous `*`, `**` or `&` holds arguments no local reads back.
    next RoundhouseShapeObserver.refuse("#{tp.self}.#{tp.method_id}: an anonymous `#{kind}` parameter") unless pname
    value = tp.binding.local_variable_get(pname)
    case kind
    when :req, :opt
      text = value.is_a?(Proc) ? RoundhouseShapeObserver.lambda_literal(tp.method_id, value) : RoundhouseShapeObserver.literal(value)
      text ? (args << text) : false
    when :rest
      texts = value.map { |v| v.is_a?(Proc) ? RoundhouseShapeObserver.lambda_literal(tp.method_id, v) : RoundhouseShapeObserver.literal(v) }
      texts.all? ? args.concat(texts) : false
    when :keyrest
      value.empty? || (t = RoundhouseShapeObserver.literal(value)) && args << "**#{t}"
    when :key, :keyreq
      (t = RoundhouseShapeObserver.literal(value)) && args << "#{pname}: #{t}"
    when :block
      value.nil? ? true : RoundhouseShapeObserver.refuse("#{tp.self}.#{tp.method_id}: handed a block")
    else true
    end
  end
  text = "#{tp.method_id} #{args.join(", ")}"
  ok ? RoundhouseShapeObserver.record(tp.self, text, text, [caller_frame.path, caller_frame.lineno]) : RoundhouseShapeObserver.refuse("#{tp.self}.#{tp.method_id}: an argument is not a literal")
end.enable
at_exit { RoundhouseShapeObserver.write }

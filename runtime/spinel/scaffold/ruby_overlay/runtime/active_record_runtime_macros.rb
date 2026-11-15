# frozen_string_literal: true
# Not lowered into the lifecycle methods as the app's own are: a gem's `included(base)` hook registers these as it runs (aasm's `base.after_initialize`).
module ActiveRecord
  class Base
    RUNTIME_CALLBACKS = %i[
      after_initialize before_validation validate after_validation
      before_save after_save before_create after_create before_update after_update
      before_destroy after_destroy after_commit
    ].freeze

    RUNTIME_CALLBACKS.each do |kind|
      define_singleton_method(kind) do |*names, **options, &block|
        __register_runtime_callback(kind, names, options, block)
      end
    end

    def self.__register_runtime_callback(kind, names, options, block)
      unknown = options.keys - %i[if unless]
      raise ArgumentError, "roundhouse: #{kind} option #{unknown.inspect} is not supported at run time" unless unknown.empty?
      entries = names.map { |name| [name, options] }
      entries << [block, options] if block
      @__runtime_callbacks ||= Hash.new { |h, k| h[k] = [] }
      # Rails replaces a symbol registered again, with the later options.
      @__runtime_callbacks[kind].reject! { |callback, _| callback.is_a?(Symbol) && names.include?(callback) }
      @__runtime_callbacks[kind].concat(entries)
      __runtime_callbacks_module.module_eval do
        next if method_defined?(kind == :after_initialize ? :initialize : kind, false)
        if kind == :after_initialize
          # Not through the record's own `after_initialize`: the constructor calls it only when the app declares one.
          define_method(:initialize) do |*args, **kwargs, &blk|
            # The outermost wrapper runs the chain once the constructor is done; an ancestor's, reached through
            # `super`, only passes on.
            outer = !@__runtime_initializing
            @__runtime_initializing = true
            super(*args, **kwargs, &blk)
            return unless outer
            @__runtime_initializing = false
            # Not on a loaded record's empty shell: the hydration factory runs them once its columns are set.
            __run_runtime_callbacks(:after_initialize) unless Thread.current[:__roundhouse_hydrating]
          end
        else
          # Rails runs callbacks in declaration order, and a gem's `include` comes before the app's own.
          define_method(kind) do
            __run_runtime_callbacks_once(kind) { super() }
          end
        end
      end
    end

    # Rails' `scope`: the body runs on the class, and a nil or false answer is the whole table.
    def self.scope(name, body, &block)
      raise ArgumentError, "roundhouse: a scope's extension block is not supported at run time" if block
      define_singleton_method(name) do |*args, **kwargs|
        instance_exec(*args, **kwargs, &body) || all
      end
    end

    def self.__runtime_callbacks_module
      @__runtime_callbacks_module ||= Module.new.tap do |mod|
        prepend(mod)
        singleton_class.prepend(__runtime_hydration_module)
      end
    end

    # Rails fires `after_initialize` on a loaded record after its columns are set, as `from_row` / `from_stmt`
    # set them after `new`.
    def self.__runtime_hydration_module
      Module.new do
        %i[from_row from_stmt].each do |factory|
          define_method(factory) do |*args, **kwargs, &blk|
            was = Thread.current[:__roundhouse_hydrating]
            Thread.current[:__roundhouse_hydrating] = true
            record = begin
              super(*args, **kwargs, &blk)
            ensure
              Thread.current[:__roundhouse_hydrating] = was
            end
            record.__run_runtime_callbacks(:after_initialize) if record.is_a?(ActiveRecord::Base) && !was
            record
          end
        end
      end
    end

    def self.__runtime_callbacks_for(kind)
      own = @__runtime_callbacks ? @__runtime_callbacks[kind] : []
      superclass.respond_to?(:__runtime_callbacks_for) ? superclass.__runtime_callbacks_for(kind) + own : own
    end

    # The subclass's wrapper runs the whole chain, its ancestors' entries first; an ancestor's own wrapper, reached
    # through `super`, only passes on, or its entries would run again.
    def __run_runtime_callbacks_once(kind)
      running = (@__runtime_callbacks_running ||= [])
      return yield if running.include?(kind)
      running << kind
      begin
        __run_runtime_callbacks(kind)
        yield
      ensure
        running.delete(kind)
      end
    end

    def __run_runtime_callbacks(kind)
      chain = self.class.__runtime_callbacks_for(kind)
      # An ancestor's symbol registered again below is the one entry, at its later place.
      chain = chain.each_with_index.reject { |(cb, _), i| cb.is_a?(Symbol) && chain[(i + 1)..].any? { |(later, _)| later == cb } }.map(&:first)
      chain.each do |callback, options|
        next unless __runtime_callback_condition(options)
        __call_runtime_callback(callback, kind)
      end
    end

    # Rails hands a Proc that takes an argument the record, runs one that takes none on it, and calls a callback
    # object's method of the callback's name with the record.
    def __call_runtime_callback(callback, kind = nil)
      return send(callback) if callback.is_a?(Symbol)
      return callback.public_send(kind, self) unless callback.is_a?(Proc) || kind.nil?
      callback.arity.zero? ? instance_exec(&callback) : instance_exec(self, &callback)
    end

    def __runtime_callback_condition(options)
      check = ->(c) { __call_runtime_callback(c) }
      Array(options[:if]).all?(&check) && Array(options[:unless]).none?(&check)
    end
  end
end

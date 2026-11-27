# frozen_string_literal: true

# ActionDispatch::ContentSecurityPolicy — the header Rails' middleware
# writes after the controller returns.
#
# Campfire's `InlineScriptHashes` narrows `script-src` on the request's
# policy during a before_action, including when an around_action serves
# a cached page and never yields. The header is applied after that
# around filter returns, which is where Rails' middleware runs. A
# request with no policy (the framework default, or
# DISABLE_CONTENT_SECURITY_POLICY) writes nothing.
module ActionDispatch
  class ContentSecurityPolicy
    def initialize
      @names = []
      @values = []
    end

    # The concern reads and replaces `policy.directives["script-src"]`.
    # Returning self keeps `[]` / `[]=` on this object; the stored value
    # is the source list, which is what `sources - ["'self'"]` expects.
    def directives
      self
    end

    def [](name)
      i = find_name(name.to_s)
      return nil if i < 0
      @values[i]
    end

    def []=(name, value)
      key = name.to_s
      i = find_name(key)
      if i < 0
        @names << key
        @values << value
      else
        @values[i] = value
      end
      nil
    end

    # Synthesized boot code calls this with the sources already quoted
    # (`"'self'"`, `"'none'"`). One directive, replaced wholesale, which
    # is what `policy.script_src :self` does.
    def set_directive(name, sources)
      self.[]=(name, sources)
      nil
    end

    def worker_src(source)
      self.[]=("worker-src", [source])
      nil
    end

    def build
      build_in(nil, nil, [])
    end

    # A source a controller's `content_security_policy` block gave as a lambda: Rails runs it in the controller as it
    # builds the header, so it is held as this marker and asked of the controller then. No source token holds `{`.
    DYNAMIC_PREFIX = "{csp-source:"

    def self.dynamic_source(index)
      DYNAMIC_PREFIX + index.to_s + "}"
    end

    # The header, each dynamic source answered by `controller` (none without one), and `'nonce-…'` after the sources
    # of each of `nonce_directives` when the request has a nonce, as Rails' `build_directives` writes them.
    def build_in(controller, nonce, nonce_directives)
      n = @names.length
      return "" if n == 0
      i = 0
      out = +""
      while i < n
        out = out + "; " if i > 0
        name = @names[i].to_s
        out = out + name + " " + join_sources(resolve_sources(@values[i], controller))
        out = out + " 'nonce-" + nonce + "'" if !nonce.nil? && nonce_directives.include?(name)
        i += 1
      end
      out
    end

    def resolve_sources(list, controller)
      return [] if list.nil?
      out = []
      list.each do |item|
        source = item.to_s
        if source.start_with?(DYNAMIC_PREFIX) && !controller.nil?
          out << controller.content_security_policy_source(source[DYNAMIC_PREFIX.length, source.length - DYNAMIC_PREFIX.length - 1].to_i).to_s
        else
          out << source
        end
      end
      out
    end

    def dup
      copy = ContentSecurityPolicy.new
      i = 0
      n = @names.length
      while i < n
        copy.set_directive(@names[i].to_s, copy_sources(@values[i]))
        i += 1
      end
      copy
    end

    # Rails' middleware: not on a 304 (a cached page's nonces would not match), nor over a header already set.
    def self.write_header(controller)
      req = controller.request
      return nil if req.nil?
      return nil if controller.status == 304 || !controller.headers["Content-Security-Policy"].nil?
      policy = req.content_security_policy
      return nil if policy.nil?
      built = policy.build_in(controller, req.content_security_policy_nonce, Rails.application.content_security_policy_nonce_directives)
      return nil if built.empty?
      controller.headers["Content-Security-Policy"] = built
      nil
    end

    def find_name(name)
      i = 0
      n = @names.length
      while i < n
        return i if @names[i] == name
        i += 1
      end
      -1
    end

    def copy_sources(list)
      return [] if list.nil?
      out = []
      i = 0
      n = list.length
      while i < n
        item = list[i]
        out << item.to_s unless item.nil?
        i += 1
      end
      out
    end

    def join_sources(list)
      return "" if list.nil?
      n = list.length
      return "" if n == 0
      i = 0
      out = +""
      while i < n
        out = out + " " if i > 0
        item = list[i]
        out = out + item.to_s unless item.nil?
        i += 1
      end
      out
    end
  end
end

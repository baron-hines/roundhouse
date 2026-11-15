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
      @__runtime_callbacks[kind].concat(entries)
      __runtime_callbacks_module.module_eval do
        next if method_defined?(kind == :after_initialize ? :initialize : kind, false)
        if kind == :after_initialize
          # Not through the record's own `after_initialize`: the constructor calls it only when the app declares one.
          define_method(:initialize) do |*args, **kwargs, &blk|
            super(*args, **kwargs, &blk)
            __run_runtime_callbacks(:after_initialize)
          end
        else
          # Rails runs callbacks in declaration order, and a gem's `include` comes before the app's own.
          define_method(kind) do
            __run_runtime_callbacks(kind)
            super()
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
      @__runtime_callbacks_module ||= Module.new.tap { |mod| prepend(mod) }
    end

    def self.__runtime_callbacks_for(kind)
      own = @__runtime_callbacks ? @__runtime_callbacks[kind] : []
      superclass.respond_to?(:__runtime_callbacks_for) ? superclass.__runtime_callbacks_for(kind) + own : own
    end

    def __run_runtime_callbacks(kind)
      self.class.__runtime_callbacks_for(kind).each do |callback, options|
        next unless __runtime_callback_condition(options)
        callback.is_a?(Proc) ? instance_exec(&callback) : send(callback)
      end
    end

    def __runtime_callback_condition(options)
      check = ->(c) { c.is_a?(Proc) ? instance_exec(&c) : send(c) }
      Array(options[:if]).all?(&check) && Array(options[:unless]).none?(&check)
    end
  end
end

# frozen_string_literal: true
# Rails 8.1's `ActionView::Helpers::Tags` for the collection inputs a gem's builder subclasses (simple_form's
# `CollectionRadioButtons < ActionView::Helpers::Tags::CollectionRadioButtons`), and the view-context helpers they
# call back on `@template_object`. CRuby overlay, beside `action_view_form_builder.rb`.
module ActionView
  class Base
    def field_id(object_name, method_name, *suffixes, index: nil, namespace: nil)
      object_name = object_name.model_name.singular if object_name.respond_to?(:model_name)
      sanitized_object_name = object_name.to_s.gsub(/\]\[|[^-a-zA-Z0-9:.]/, "_").delete_suffix("_")
      [namespace, (sanitized_object_name unless sanitized_object_name.empty?), (index unless sanitized_object_name.empty?),
       method_name.to_s.delete_suffix("?"), *suffixes].compact.join("_")
    end

    def field_name(object_name, method_name, *method_names, multiple: false, index: nil)
      names = method_names.map { |name| "[#{name}]" }.join
      if object_name.nil? || object_name.to_s.empty?
        "#{method_name}#{names}#{multiple ? "[]" : ""}"
      elsif index
        "#{object_name}[#{index}][#{method_name}]#{names}#{multiple ? "[]" : ""}"
      else
        "#{object_name}[#{method_name}]#{names}#{multiple ? "[]" : ""}"
      end
    end

    def label(object_name, method, content_or_options = nil, options = nil, &block)
      view_builder(object_name, options || (content_or_options.is_a?(Hash) ? content_or_options : {})).label(method, content_or_options, options || {}, &block)
    end

    def radio_button(object_name, method, tag_value, options = {})
      view_builder(object_name, options).radio_button(method, tag_value, options)
    end

    def checkbox(object_name, method, options = {}, checked_value = "1", unchecked_value = "0")
      view_builder(object_name, options).check_box(method, options, checked_value, unchecked_value)
    end
    alias check_box checkbox

    def hidden_field_tag(name, value = nil, options = {})
      tag(:input, { "type" => "hidden", "name" => name, "id" => sanitize_to_id(name), "value" => value, "autocomplete" => "off" }.merge(Helpers::FormBuilder.stringify(options)))
    end

    def label_tag(name = nil, content_or_options = nil, options = nil, &block)
      if block && content_or_options.is_a?(Hash)
        options = content_or_options
      else
        options ||= {}
      end
      opts = Helpers::FormBuilder.stringify(options)
      opts["for"] = sanitize_to_id(name) unless name.nil? || name.to_s.empty? || opts.key?("for")
      content_tag(:label, block ? capture(&block) : (content_or_options.is_a?(Hash) ? nil : content_or_options) || ActiveSupport.humanize(name.to_s), opts)
    end

    private

    # Rails' `sanitize_to_id`: `]` dropped, each other non-id character an underscore.
    def sanitize_to_id(name)
      name.to_s.delete("]").tr("^-a-zA-Z0-9:.", "_")
    end

    def view_builder(object_name, options)
      Helpers::FormBuilder.new(object_name, options.is_a?(Hash) ? options.delete(:object) : nil, self, {})
    end
  end

  module Helpers
    module FormOptionsHelper
      private

      def option_html_attributes(element)
        element.is_a?(Array) ? element.select { |e| e.is_a?(Hash) }.reduce({}, :merge!) : {}
      end

      def value_for_collection(item, value)
        value.respond_to?(:call) ? value.call(item) : item.public_send(value)
      end
    end

    module Tags
      class Base
        attr_reader :object

        def initialize(object_name, method_name, template_object, options = {})
          @object_name, @method_name = object_name.to_s.dup, method_name.to_s.dup
          @template_object = template_object
          @object_name.sub!(/\[\]$/, "") || @object_name.sub!(/\[\]\]$/, "]")
          @object = retrieve_object(options.delete(:object))
          @skip_default_ids = options.delete(:skip_default_ids)
          @allow_method_names_outside_object = options.delete(:allow_method_names_outside_object)
          @options = options
          if Regexp.last_match
            @generate_indexed_names = true
            @auto_index = retrieve_autoindex(Regexp.last_match.pre_match)
          else
            @generate_indexed_names = false
            @auto_index = nil
          end
        end

        def render
          raise NotImplementedError, "Subclasses must implement a render method"
        end

        private

        def tag(name, options)
          @template_object.tag(name, options)
        end

        def value
          return unless object
          if @allow_method_names_outside_object
            object.public_send @method_name if object.respond_to?(@method_name)
          else
            object.public_send @method_name
          end
        end

        def retrieve_object(object)
          if object
            object
          elsif @template_object.instance_variable_defined?("@#{@object_name}")
            @template_object.instance_variable_get("@#{@object_name}")
          end
        rescue NameError
          nil
        end

        def retrieve_autoindex(pre_match)
          object = self.object || @template_object.instance_variable_get("@#{pre_match}")
          raise ArgumentError, "object[] naming but object param and @object var don't exist or don't respond to to_param: #{object.inspect}" unless object.respond_to?(:to_param)
          object.to_param
        end

        def add_default_name_and_field_for_value(tag_value, options, field = "id")
          if tag_value.nil?
            add_default_name_and_field(options, field)
          else
            specified_field = options[field]
            add_default_name_and_field(options, field)
            options[field] += "_#{sanitized_value(tag_value)}" if (specified_field.nil? || specified_field.to_s.strip.empty?) && !options[field].to_s.strip.empty?
          end
        end
        alias add_default_name_and_id_for_value add_default_name_and_field_for_value

        def add_default_name_and_field(options, field = "id")
          index = name_and_id_index(options)
          options["name"] = options.fetch("name") { tag_name(options["multiple"], index) }
          return unless generate_ids?
          options[field] = options.fetch(field) { tag_id(index, options.delete("namespace")) }
          if (namespace = options.delete("namespace"))
            options[field] = options[field] ? "#{namespace}_#{options[field]}" : namespace
          end
        end
        alias add_default_name_and_id add_default_name_and_field

        def tag_name(multiple = false, index = nil)
          @template_object.field_name(@object_name, sanitized_method_name, multiple: multiple, index: index)
        end

        def tag_id(index = nil, namespace = nil)
          @template_object.field_id(@object_name, @method_name, index: index, namespace: namespace)
        end

        def sanitized_method_name
          @sanitized_method_name ||= @method_name.delete_suffix("?")
        end

        def sanitized_value(value)
          value.to_s.gsub(/[\s.]/, "_").gsub(/[^-[[:word:]]]/, "").downcase
        end

        def name_and_id_index(options)
          if options.key?("index")
            options.delete("index") || ""
          elsif @generate_indexed_names
            @auto_index || ""
          end
        end

        def generate_ids?
          !@skip_default_ids
        end
      end

      module CollectionHelpers
        class Builder
          attr_reader :object, :text, :value

          def initialize(template_object, object_name, method_name, object, sanitized_attribute_name, text, value, input_html_options)
            @template_object = template_object
            @object_name = object_name
            @method_name = method_name
            @object = object
            @sanitized_attribute_name = sanitized_attribute_name
            @text = text
            @value = value
            @input_html_options = input_html_options
          end

          def label(label_html_options = {}, &block)
            html_options = @input_html_options.slice(:index, :namespace).merge(label_html_options)
            html_options[:for] ||= @input_html_options[:id] if @input_html_options[:id]
            @template_object.label(@object_name, @sanitized_attribute_name, @text, html_options, &block)
          end
        end

        def initialize(object_name, method_name, template_object, collection, value_method, text_method, options, html_options)
          @collection = collection
          @value_method = value_method
          @text_method = text_method
          @html_options = html_options
          super(object_name, method_name, template_object, options)
        end

        private

        def instantiate_builder(builder_class, item, value, text, html_options)
          builder_class.new(@template_object, @object_name, @method_name, item, sanitize_attribute_name(value), text, value, html_options)
        end

        def default_html_options_for_collection(item, value)
          html_options = @html_options.dup
          [:checked, :selected, :disabled, :readonly].each do |option|
            current_value = @options[option]
            next if current_value.nil?
            accept = current_value.respond_to?(:call) ? current_value.call(item) : Array(current_value).map(&:to_s).include?(value.to_s)
            if accept
              html_options[option] = true
            elsif option == :checked
              html_options[option] = false
            end
          end
          html_options[:object] = @object
          html_options
        end

        def sanitize_attribute_name(value)
          sanitized = sanitized_value(value)
          sanitized.empty? ? sanitized_method_name.dup : "#{sanitized_method_name}_#{sanitized}"
        end

        def render_collection
          @collection.map do |item|
            value = value_for_collection(item, @value_method)
            text = value_for_collection(item, @text_method)
            yield item, value, text, default_html_options_for_collection(item, value).merge(option_html_attributes(item))
          end.join.html_safe
        end

        def render_collection_for(builder_class, &block)
          options = @options.transform_keys(&:to_s)
          rendered_collection = render_collection do |item, value, text, default_html_options|
            builder = instantiate_builder(builder_class, item, value, text, default_html_options)
            block ? @template_object.capture(builder, &block) : render_component(builder)
          end
          options.fetch("include_hidden", true) ? hidden_field + rendered_collection : rendered_collection
        end

        def hidden_field
          @template_object.hidden_field_tag(@html_options[:name] || hidden_field_name, "", { id: nil, form: @html_options[:form] })
        end

        def hidden_field_name
          tag_name(false, @options[:index]).to_s
        end
      end

      class CollectionRadioButtons < Base
        include CollectionHelpers
        include FormOptionsHelper

        class RadioButtonBuilder < Builder
          def radio_button(extra_html_options = {})
            html_options = extra_html_options.merge(@input_html_options)
            html_options[:skip_default_ids] = false
            @template_object.radio_button(@object_name, @method_name, @value, html_options)
          end
        end

        def render(&block)
          render_collection_for(RadioButtonBuilder, &block)
        end

        private

        def render_component(builder)
          builder.radio_button + builder.label
        end
      end

      class CollectionCheckBoxes < Base
        include CollectionHelpers
        include FormOptionsHelper

        class CheckBoxBuilder < Builder
          def checkbox(extra_html_options = {})
            html_options = extra_html_options.merge(@input_html_options)
            html_options[:multiple] = true
            html_options[:skip_default_ids] = false
            @template_object.checkbox(@object_name, @method_name, html_options, @value, nil)
          end
          alias check_box checkbox
        end

        def render(&block)
          render_collection_for(CheckBoxBuilder, &block)
        end

        private

        def render_component(builder)
          builder.checkbox + builder.label
        end

        def hidden_field_name
          "#{super}[]"
        end
      end
    end
  end
end

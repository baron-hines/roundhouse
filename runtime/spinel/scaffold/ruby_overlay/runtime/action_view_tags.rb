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

    # Rails' FormHelper: each field the `Tags::*` class of its name renders.
    { text_field: "TextField", password_field: "PasswordField", hidden_field: "HiddenField", email_field: "EmailField",
      url_field: "UrlField", telephone_field: "TelField", search_field: "SearchField", color_field: "ColorField",
      number_field: "NumberField", range_field: "RangeField", date_field: "DateField", time_field: "TimeField",
      datetime_field: "DatetimeLocalField", datetime_local_field: "DatetimeLocalField", month_field: "MonthField",
      week_field: "WeekField", text_area: "TextArea" }.each do |helper, tag|
      define_method(helper) { |object_name, method, options = {}| Helpers::Tags.const_get(tag).new(object_name, method, self, options).render }
    end
    alias phone_field telephone_field
    alias textarea text_area

    def label(object_name, method, content_or_options = nil, options = nil, &block)
      Helpers::Tags::Label.new(object_name, method, self, content_or_options, options).render(&block)
    end

    def radio_button(object_name, method, tag_value, options = {})
      Helpers::Tags::RadioButton.new(object_name, method, self, tag_value, options).render
    end

    def select(object, method, choices = nil, options = {}, html_options = {}, &block)
      Helpers::Tags::Select.new(object, method, self, choices, options, html_options, &block).render
    end

    def collection_select(object, method, collection, value_method, text_method, options = {}, html_options = {})
      Helpers::Tags::CollectionSelect.new(object, method, self, collection, value_method, text_method, options, html_options).render
    end

    def checkbox(object_name, method, options = {}, checked_value = "1", unchecked_value = "0")
      Helpers::Tags::CheckBox.new(object_name, method, self, checked_value, unchecked_value, options).render
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

  end

  module Helpers
    module FormOptionsHelper
      # Rails' `options_for_select`: an `<option>` per element, its text and value the pair's (or the element twice).
      def options_for_select(container, selected = nil)
        return container if String === container

        selected, disabled = extract_selected_and_disabled(selected).map { |r| Array(r).map(&:to_s) }
        container.map do |element|
          html_attributes = option_html_attributes(element)
          text, value = option_text_and_value(element).map(&:to_s)
          html_attributes[:selected] ||= option_value_selected?(value, selected)
          html_attributes[:disabled] ||= disabled && option_value_selected?(value, disabled)
          html_attributes[:value] = value
          option_tag(text, html_attributes)
        end.join("\n").html_safe
      end

      def options_from_collection_for_select(collection, value_method, text_method, selected = nil)
        options = collection.map do |element|
          [value_for_collection(element, text_method), value_for_collection(element, value_method), option_html_attributes(element)]
        end
        selected, disabled = extract_selected_and_disabled(selected)
        select_deselect = {
          selected: extract_values_from_collection(collection, value_method, selected),
          disabled: extract_values_from_collection(collection, value_method, disabled)
        }
        options_for_select(options, select_deselect)
      end

      def grouped_options_for_select(grouped_options, selected_key = nil, options = {})
        prompt = options[:prompt]
        divider = options[:divider]
        body = "".html_safe
        body.safe_concat(option_tag(prompt_text(prompt), { value: "" })) if prompt
        grouped_options.each do |container|
          html_attributes = option_html_attributes(container)
          if divider
            label = divider
          else
            label, container = container
          end
          html_attributes = { label: label }.merge!(html_attributes)
          body.safe_concat(ActionView::Helpers::FormBuilder.tag_html("optgroup", options_for_select(container, selected_key), html_attributes))
        end
        body
      end

      private

      # Rails' `tag_builder.option`, which no field error wraps.
      def option_tag(text, attributes)
        ActionView::Helpers::FormBuilder.tag_html("option", text, attributes)
      end

      def option_text_and_value(option)
        if !option.is_a?(String) && option.respond_to?(:first) && option.respond_to?(:last)
          option = option.reject { |e| Hash === e } if Array === option
          [option.first, option.last]
        else
          [option, option]
        end
      end

      def option_value_selected?(value, selected)
        Array(selected).include?(value)
      end

      def extract_selected_and_disabled(selected)
        if selected.is_a?(Proc)
          [selected, nil]
        else
          selected = selected.is_a?(Array) ? selected.dup : [selected].compact
          options = (selected.last.is_a?(Hash) ? selected.pop : {}).transform_keys(&:to_sym)
          [options.fetch(:selected, selected), options[:disabled]]
        end
      end

      def extract_values_from_collection(collection, value_method, selected)
        if selected.is_a?(Proc)
          collection.filter_map { |element| element.public_send(value_method) if selected.call(element) }
        else
          selected
        end
      end

      def prompt_text(prompt)
        prompt.is_a?(String) ? prompt : I18n.translate("helpers.select.prompt", default: "Please select")
      end

      def option_html_attributes(element)
        element.is_a?(Array) ? element.select { |e| e.is_a?(Hash) }.reduce({}, :merge!) : {}
      end

      def value_for_collection(item, value)
        value.respond_to?(:call) ? value.call(item) : item.public_send(value)
      end
    end

    module Tags
      class Base
        def object
          @active_model_object ||= @object.respond_to?(:to_model) ? @object.to_model : @object
        end

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

        # Rails' ActiveModelInstanceTag: a field of an attribute with errors runs `field_error_proc`, a hidden one not.
        def tag(name, options)
          html = @template_object.tag(name, options)
          options["type"] == "hidden" ? html : error_wrapping(html)
        end

        def content_tag(name, content, options)
          error_wrapping(@template_object.content_tag(name, content, options))
        end

        def error_wrapping(html_tag)
          return html_tag unless object_has_errors?
          @template_object.instance_exec(html_tag, self, &ActionView::Base.field_error_proc)
        end

        def error_message
          object.errors[@method_name]
        end

        def object_has_errors?
          object.respond_to?(:errors) && object.errors.respond_to?(:[]) && error_message.present?
        end

        def value_before_type_cast
          return unless object
          method_before_type_cast = @method_name + "_before_type_cast"
          if value_came_from_user? && object.respond_to?(method_before_type_cast)
            object.public_send(method_before_type_cast)
          else
            value
          end
        end

        def value_came_from_user?
          method_name = "#{@method_name}_came_from_user?"
          !object.respond_to?(method_name) || object.public_send(method_name)
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

      # Rails' `Tags::Placeholderable`: `placeholder: true` is the attribute's human name.
      module Placeholderable
        def initialize(*)
          super
          tag_value = @options[:placeholder]
          return unless tag_value
          @options[:placeholder] = tag_value.is_a?(String) ? tag_value : Translator.new(object, @object_name, tag_value.is_a?(TrueClass) ? @method_name : "#{@method_name}.#{tag_value}").translate
        end
      end

      # Rails' `Tags::Translator`, by the model's `human_attribute_name`; the `helpers.label` I18n lookup is not modeled.
      class Translator
        def initialize(object, object_name, method_and_value)
          @model = object.respond_to?(:to_model) ? object.to_model : nil
          @method_and_value = method_and_value
        end

        def translate
          name = @model.class.human_attribute_name(@method_and_value) if @model && @model.class.respond_to?(:human_attribute_name)
          name || ActiveSupport.humanize(@method_and_value.to_s)
        end
      end

      class TextField < Base
        include Placeholderable

        def self.field_type
          @field_type ||= name.split("::").last.sub("Field", "").downcase
        end

        def render
          options = @options.transform_keys(&:to_s)
          options["size"] = options["maxlength"] unless options.key?("size")
          options["type"] ||= field_type
          options["value"] = options.fetch("value") { value_before_type_cast } unless field_type == "file"
          add_default_name_and_field(options)
          tag("input", options)
        end

        private

        def field_type
          self.class.field_type
        end
      end

      class PasswordField < TextField
        def render
          @options = { value: nil }.merge!(@options)
          super
        end
      end

      class HiddenField < TextField
        def render
          @options = { autocomplete: "off" }.merge(@options)
          super
        end
      end

      class EmailField < TextField; end
      class UrlField < TextField; end
      class TelField < TextField; end
      class SearchField < TextField; end
      class ColorField < TextField; end

      class NumberField < TextField
        def render
          options = @options.transform_keys(&:to_s)
          if (range = options.delete("in") || options.delete("within"))
            options.update("min" => range.min, "max" => range.max)
          end
          @options = options
          super
        end
      end

      class RangeField < NumberField; end

      class DatetimeField < TextField
        def render
          options = @options.transform_keys(&:to_s)
          options["value"] = datetime_value(options.fetch("value") { value })
          options["min"] = format_datetime(parse_datetime(options["min"]))
          options["max"] = format_datetime(parse_datetime(options["max"]))
          @options = options
          super
        end

        private

        def datetime_value(value)
          value.is_a?(String) ? value : format_datetime(value)
        end

        def parse_datetime(value)
          value.is_a?(String) ? (DateTime.parse(value) rescue nil) : value
        end
      end

      class DateField < DatetimeField
        private def format_datetime(value) = value&.strftime("%Y-%m-%d")
      end

      class DatetimeLocalField < DatetimeField
        def self.field_type = "datetime-local"

        def initialize(object_name, method_name, template_object, options = {})
          @include_seconds = options.delete(:include_seconds) { true }
          super
        end

        private def format_datetime(value) = value&.strftime(@include_seconds ? "%Y-%m-%dT%T" : "%Y-%m-%dT%H:%M")
      end

      class TimeField < DatetimeField
        def initialize(object_name, method_name, template_object, options = {})
          @include_seconds = options.delete(:include_seconds) { true }
          super
        end

        private def format_datetime(value) = value&.strftime(@include_seconds ? "%T.%L" : "%H:%M")
      end

      class MonthField < DatetimeField
        private def format_datetime(value) = value&.strftime("%Y-%m")
      end

      class WeekField < DatetimeField
        private def format_datetime(value) = value&.strftime("%G-W%V")
      end

      class TextArea < Base
        include Placeholderable

        def render
          options = @options.transform_keys(&:to_s)
          add_default_name_and_field(options)
          if (size = options.delete("size")) && size.respond_to?(:split)
            options["cols"], options["rows"] = size.split("x")
          end
          content_tag("textarea", options.delete("value") { value_before_type_cast }, options)
        end
      end

      module Checkable
        def input_checked?(options)
          if options.key?("checked")
            checked = options.delete("checked")
            checked == true || checked == "checked"
          else
            checked?(value)
          end
        end
      end

      class CheckBox < Base
        include Checkable

        def initialize(object_name, method_name, template_object, checked_value, unchecked_value, options)
          @checked_value = checked_value
          @unchecked_value = unchecked_value
          super(object_name, method_name, template_object, options)
        end

        def render
          options = @options.transform_keys(&:to_s)
          options["type"] = "checkbox"
          options["value"] = @checked_value
          options["checked"] = "checked" if input_checked?(options)
          if options["multiple"]
            add_default_name_and_field_for_value(@checked_value, options)
            options.delete("multiple")
          else
            add_default_name_and_field(options)
          end
          include_hidden = options.key?("include_hidden") ? options.delete("include_hidden") : true
          checkbox = tag("input", options)
          include_hidden ? (hidden_field_for_checkbox(options) + checkbox).html_safe : checkbox
        end

        private

        def checked?(value)
          case value
          when TrueClass, FalseClass then value == !!@checked_value
          when NilClass then false
          when String then value == @checked_value
          else
            value.respond_to?(:include?) ? value.include?(@checked_value) : value.to_i == @checked_value.to_i
          end
        end

        def hidden_field_for_checkbox(options)
          return "".html_safe unless @unchecked_value
          tag("input", options.slice("name", "disabled", "form").merge!("type" => "hidden", "value" => @unchecked_value, "autocomplete" => "off"))
        end
      end

      # Rails 8.1's `Tags::SelectRenderer`.
      module SelectRenderer
        private

        def select_content_tag(option_tags, options, html_options)
          html_options = html_options.to_h.transform_keys(&:to_s)
          [:required, :multiple, :size].each do |prop|
            html_options[prop.to_s] = options.delete(prop) if options.key?(prop) && !html_options.key?(prop.to_s)
          end
          add_default_name_and_field(html_options)
          if placeholder_required?(html_options)
            raise ArgumentError, "include_blank cannot be false for a required field." if options[:include_blank] == false
            options[:include_blank] ||= true unless options[:prompt]
          end
          value = options.fetch(:selected) { value() }
          select = content_tag("select", add_options(option_tags, options, value), html_options)
          if html_options["multiple"] && options.fetch(:include_hidden, true)
            hidden = { "disabled" => html_options["disabled"], "name" => html_options["name"], "type" => "hidden", "value" => "", "autocomplete" => "off" }
            @template_object.tag("input", hidden) + select
          else
            select
          end
        end

        def placeholder_required?(html_options)
          html_options["required"] && !html_options["multiple"] && html_options.fetch("size", 1).to_i == 1
        end

        def add_options(option_tags, options, value = nil)
          if options[:include_blank]
            content = (options[:include_blank] if options[:include_blank].is_a?(String))
            label = (" " unless content)
            option_tags = option_tag(content, { value: "", label: label }) + "\n" + option_tags
          end
          if value.blank? && options[:prompt]
            tag_options = { value: "" }
            tag_options[:disabled] = true if options[:disabled] == ""
            tag_options[:selected] = true if options[:selected] == ""
            option_tags = option_tag(prompt_text(options[:prompt]), tag_options) + "\n" + option_tags
          end
          option_tags
        end
      end

      class Select < Base
        include SelectRenderer
        include FormOptionsHelper

        def initialize(object_name, method_name, template_object, choices, options, html_options)
          @choices = block_given? ? template_object.capture { yield || "" } : choices
          @choices = @choices.to_a if @choices.is_a?(Range)
          @html_options = html_options
          super(object_name, method_name, template_object, options)
        end

        def render
          option_tags_options = { selected: @options.fetch(:selected) { value.nil? ? "" : value }, disabled: @options[:disabled] }
          option_tags = if grouped_choices?
            grouped_options_for_select(@choices, option_tags_options)
          else
            options_for_select(@choices, option_tags_options)
          end
          select_content_tag(option_tags, @options, @html_options)
        end

        private

        def grouped_choices?
          return false if @choices.blank?
          first_choice = @choices.first
          return false unless first_choice.is_a?(Enumerable)
          first_choice.to_a[1].is_a?(Array)
        end
      end

      class CollectionSelect < Base
        include SelectRenderer
        include FormOptionsHelper

        def initialize(object_name, method_name, template_object, collection, value_method, text_method, options, html_options)
          @collection = collection
          @value_method = value_method
          @text_method = text_method
          @html_options = html_options
          super(object_name, method_name, template_object, options)
        end

        def render
          option_tags_options = { selected: @options.fetch(:selected) { value }, disabled: @options[:disabled] }
          select_content_tag(
            options_from_collection_for_select(@collection, @value_method, @text_method, option_tags_options),
            @options, @html_options
          )
        end
      end

      class RadioButton < Base
        include Checkable

        def initialize(object_name, method_name, template_object, tag_value, options)
          @tag_value = tag_value
          super(object_name, method_name, template_object, options)
        end

        def render
          options = @options.transform_keys(&:to_s)
          options["type"] = "radio"
          options["value"] = @tag_value
          options["checked"] = "checked" if input_checked?(options)
          add_default_name_and_field_for_value(@tag_value, options)
          tag("input", options)
        end

        private

        def checked?(value)
          value.to_s == @tag_value.to_s
        end
      end

      class Label < Base
        class LabelBuilder
          attr_reader :object

          def initialize(template_object, object_name, method_name, object, tag_value)
            @template_object = template_object
            @object_name = object_name
            @method_name = method_name
            @object = object
            @tag_value = tag_value
          end

          def translation
            Translator.new(object, @object_name, @tag_value.present? ? "#{@method_name}.#{@tag_value}" : @method_name).translate
          end

          def to_s
            translation
          end
        end

        def initialize(object_name, method_name, template_object, content_or_options = nil, options = nil)
          options ||= {}
          if content_or_options.is_a?(Hash)
            options = options.merge(content_or_options)
            @content = nil
          else
            @content = content_or_options
          end
          super(object_name, method_name, template_object, options)
        end

        def render(&block)
          options = @options.transform_keys(&:to_s)
          tag_value = options.delete("value")
          add_default_name_and_field_for_value(tag_value, options, "for")
          options.delete("index")
          options.delete("name")
          options.delete("namespace")
          builder = LabelBuilder.new(@template_object, @object_name, @method_name, object, tag_value)
          content = if block
            @template_object.capture(builder, &block)
          elsif @content.present?
            @content.to_s
          else
            builder.translation
          end
          error_wrapping(@template_object.label_tag(options["for"], content, options))
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

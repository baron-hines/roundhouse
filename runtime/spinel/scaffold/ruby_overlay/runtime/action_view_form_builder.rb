# frozen_string_literal: true
# Rails' `ActionView::Helpers::FormBuilder` and the view-context helpers it calls through `@template`, at request
# time, for a builder the form lowering does not expand: an app's subclass (`form_with(builder: MoneyFormBuilder)`)
# or a gem's (simple_form's). The lowering still writes a stock builder's fields as markup; a builder method of
# another class runs here, and what it calls back (`text_field`, `label`, `@template.content_tag`) answers the
# markup Rails 8.1 does, marked html_safe so the template writes it as it is. CRuby overlay: the record is reached
# through `public_send` by the field's name, which the typed runtime does not carry.
module ActionView
  class Base
    # Rails' `content_tag(name, content = nil, options = nil, &block)`.
    def content_tag(name, content_or_options = nil, options = nil, &block)
      if block
        options = content_or_options if content_or_options.is_a?(Hash)
        content = capture(&block)
      else
        content = content_or_options
      end
      ActionView::Helpers::FormBuilder.tag_html(name, content, options)
    end

    # Rails' `capture`: the block's value, a String escaped unless it is html_safe.
    def capture(*args)
      value = yield(*args)
      return nil unless value.is_a?(String)
      ActionView::ViewHelpers.html_escape(value).html_safe
    end

    def safe_join(array, sep = "")
      array.flatten.map { |item| ActionView::ViewHelpers.html_escape(item.to_s) }.join(ActionView::ViewHelpers.html_escape(sep.to_s)).html_safe
    end

    # Rails' `tag(name, options)`: a void element, `<br />`.
    def tag(name, options = nil)
      "<#{ActionView::ViewHelpers.content_tag_name(name.to_s)}#{ActionView::ViewHelpers.render_attrs((options || {}).to_h)} />".html_safe
    end

    def submit_tag(value = "Save changes", options = {})
      opts = { "type" => "submit", "name" => "commit", "value" => value }.merge(ActionView::Helpers::FormBuilder.stringify(options))
      opts["data-disable-with"] = value unless opts.key?("data-disable-with") || opts.delete("data") == false
      tag(:input, opts)
    end

    def button_tag(content = nil, options = {})
      opts = { "name" => "button", "type" => "submit" }.merge(ActionView::Helpers::FormBuilder.stringify(options))
      content_tag(:button, content || "Button", opts)
    end
  end

  module Helpers
    class FormBuilder
      attr_reader :object, :object_name, :options

      def initialize(object_name, object, template, options)
        @object_name = object_name
        @object = object
        @template = template
        @options = options
      end

      def self.stringify(options)
        (options || {}).to_h.transform_keys(&:to_s)
      end

      def self.tag_html(name, content, options)
        n = ActionView::ViewHelpers.content_tag_name(name.to_s)
        body = ActionView::ViewHelpers.html_escape(content.to_s)
        "<#{n}#{ActionView::ViewHelpers.render_attrs((options || {}).to_h)}>#{ActionView::ViewHelpers.content_tag_pre(n)}#{body}</#{n}>".html_safe
      end

      # Rails' `Tags::Base#tag_id`: the name with each run of other characters an underscore.
      def field_id(method)
        "#{@object_name.to_s.gsub(/\]\[|[^-a-zA-Z0-9:.]/, "_").delete_suffix("_")}_#{method}"
      end

      def field_name(method)
        "#{@object_name}[#{method}]"
      end

      %w[text password email url telephone phone number search color range date time datetime_local month week].each do |kind|
        type = { "telephone" => "tel", "phone" => "tel", "datetime_local" => "datetime-local" }.fetch(kind, kind)
        define_method("#{kind}_field") { |method, options = {}| input_field(type, method, options) }
      end

      def hidden_field(method, options = {})
        input_field("hidden", method, { "autocomplete" => "off" }.merge(FormBuilder.stringify(options)))
      end

      def text_area(method, options = {})
        opts = FormBuilder.stringify(options)
        value = opts.key?("value") ? opts.delete("value") : value_of(method)
        opts["name"] ||= field_name(method)
        opts["id"] ||= field_id(method)
        with_errors(method, FormBuilder.tag_html(:textarea, value, opts))
      end
      alias textarea text_area

      def check_box(method, options = {}, checked_value = "1", unchecked_value = "0")
        opts = FormBuilder.stringify(options)
        hidden = unchecked_value.nil? ? "" : @template.tag(:input, "name" => field_name(method), "type" => "hidden", "value" => unchecked_value, "autocomplete" => "off")
        opts["type"] = "checkbox"
        opts["value"] = checked_value
        opts["name"] ||= field_name(method)
        opts["id"] ||= field_id(method)
        current = value_of(method)
        opts["checked"] = "checked" if current == true || (!current.nil? && current != false && current.to_s == checked_value.to_s)
        with_errors(method, (hidden + @template.tag(:input, opts)).html_safe)
      end
      alias checkbox check_box

      # Rails' `Tags::Label`: the text given, else the attribute's human name.
      def label(method, text = nil, options = {}, &block)
        options, text = text, nil if text.is_a?(Hash)
        opts = FormBuilder.stringify(options)
        content = block ? @template.capture(&block) : (text || human_attribute(method))
        opts["for"] ||= field_id(method)
        with_errors(method, FormBuilder.tag_html(:label, content, opts))
      end

      def submit(value = nil, options = {})
        options, value = value, nil if value.is_a?(Hash)
        @template.submit_tag(value || submit_default_value, options)
      end

      def button(value = nil, options = {}, &block)
        options, value = value, nil if value.is_a?(Hash)
        content = block ? @template.capture(&block) : (value || submit_default_value)
        @template.button_tag(content, options)
      end

      private

      def input_field(type, method, options)
        opts = FormBuilder.stringify(options)
        opts["size"] = opts["maxlength"] unless opts.key?("size")
        opts["type"] ||= type
        opts["value"] = opts.key?("value") ? opts["value"] : value_of(method) unless type == "password"
        opts["name"] ||= field_name(method)
        opts["id"] ||= field_id(method)
        with_errors(method, @template.tag(:input, opts))
      end

      # Rails' `value_before_type_cast`, else the reader's value.
      def value_of(method)
        return nil if @object.nil?
        cast = "#{method}_before_type_cast"
        return @object.public_send(cast) if @object.respond_to?(cast)
        @object.respond_to?(method) ? @object.public_send(method) : nil
      end

      # Rails' default `field_error_proc`.
      def with_errors(method, html)
        return html unless errors_on?(method)
        "<div class=\"field_with_errors\">#{html}</div>".html_safe
      end

      def errors_on?(method)
        return false if @object.nil?
        return @object.error_messages_for(method.to_s).any? if @object.respond_to?(:error_messages_for)
        @object.respond_to?(:errors) && @object.errors[method].any?
      end

      def human_attribute(method)
        klass = @object&.class
        return klass.human_attribute_name(method.to_s) if klass.respond_to?(:human_attribute_name)
        ActiveSupport.humanize(method.to_s)
      end

      # Rails' `submit_default_value`: Create or Update, and the model's human name.
      def submit_default_value
        return "Save changes" if @object.nil?
        model = @object.class.respond_to?(:model_name) ? @object.class.model_name.human : ActiveSupport.humanize(@object_name.to_s)
        persisted = @object.respond_to?(:persisted?) && @object.persisted?
        "#{persisted ? "Update" : "Create"} #{model}"
      end
    end
  end
end

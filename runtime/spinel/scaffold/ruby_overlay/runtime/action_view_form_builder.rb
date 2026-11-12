# frozen_string_literal: true
# Rails' `ActionView::Helpers::FormBuilder` and the view-context helpers it calls through `@template`, at request
# time, for a builder the form lowering does not expand: an app's subclass (`form_with(builder: MoneyFormBuilder)`)
# or a gem's (simple_form's). The lowering still writes a stock builder's fields as markup; a builder method of
# another class runs here, and what it calls back (`text_field`, `label`, `@template.content_tag`) answers the
# markup Rails 8.1 does, marked html_safe so the template writes it as it is. CRuby overlay: the record is reached
# through `public_send` by the field's name, which the typed runtime does not carry.
module ActionView
  class Base
    # Rails' default: the field and its label boxed, run on the view (`instance_exec`) with the field's tag.
    @field_error_proc = proc { |html_tag, _instance| "<div class=\"field_with_errors\">#{html_tag}</div>".html_safe }

    class << self
      attr_accessor :field_error_proc
    end

    # The request's controller, which Rails' view context holds (simple_form reads `template.controller.action_name`).
    def controller
      ActionController::Current.controller
    end

    def request
      ActionController::Current.request
    end

    # Rails' `form_for`: the record's name, id and class, its route, and the method its persistence says, as
    # `form_with(model:)` writes them; the block's markup between the form tag and its end.
    def form_for(record, options = {}, &block)
      raise ArgumentError, "Missing block" unless block
      html = (options[:html] || {}).dup
      if record.is_a?(String) || record.is_a?(Symbol)
        object = nil
        object_name = record
      else
        object = record.is_a?(Array) ? record.last : record
        object = object.to_model if object.respond_to?(:to_model)
        object_name = options[:as] || object.class.model_name.param_key
        as = options[:as]
        action, method = object.respond_to?(:persisted?) && object.persisted? ? [:edit, :patch] : [:new, :post]
        defaults = { class: as ? "#{action}_#{as}" : ActionView::ViewHelpers.dom_class(object, action),
                     id: (as ? [options[:namespace], action, as] : [options[:namespace], ActionView::ViewHelpers.dom_id(object, action)]).compact.join("_") }
        defaults.delete(:id) if defaults[:id].empty?
        html = defaults.merge(html)
        options = options.merge(url: options[:url] || polymorphic_path(record), method: options[:method] || method)
      end
      builder_class = options[:builder] || Helpers::FormBuilder
      builder = builder_class.new(object_name, object, self, options.except(:builder, :html, :url, :method, :as, :namespace))
      body = capture(builder, &block)
      method = (html.delete(:method) || options[:method] || :post).to_s
      html[:enctype] = "multipart/form-data" if html.delete(:multipart) || builder.multipart?
      html[:action] = options[:url].is_a?(String) ? options[:url] : polymorphic_path(options[:url])
      html[:"accept-charset"] = "UTF-8"
      html[:method] = method == "get" ? "get" : "post"
      extra = method == "get" ? "" : ActionView::ViewHelpers.method_override_input(method) + ActionView::ViewHelpers.csrf_token_hidden_input
      "<form#{ActionView::ViewHelpers.render_attrs(Helpers::FormBuilder.tag_options(html))}>#{extra}#{body}</form>".html_safe
    end

    # Rails' `polymorphic_path` for a record, a class or a nesting of them, by the app's route helpers: the
    # collection's for a new record or a class, the member's for a persisted one.
    def polymorphic_path(record_or_array)
      parts = Array(record_or_array).compact
      last = parts.last
      member = !last.is_a?(Class) && !last.is_a?(Symbol) && !last.is_a?(String) && last.respond_to?(:persisted?) && last.persisted?
      args = []
      names = parts.each_with_index.map do |part, i|
        next part.to_s if part.is_a?(Symbol) || part.is_a?(String)
        model = part.is_a?(Class) ? part.model_name : part.class.model_name
        if i < parts.size - 1 || member
          args << part.to_param
          model.singular_route_key
        else
          model.route_key
        end
      end
      RouteHelpers.public_send(:"#{names.join("_")}_path", *args)
    end

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
      "<#{ActionView::ViewHelpers.content_tag_name(name.to_s)}#{ActionView::ViewHelpers.render_attrs(Helpers::FormBuilder.tag_options(options))} />".html_safe
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
        @default_options = options.slice(:index, :namespace, :skip_default_ids, :allow_method_names_outside_object)
        @default_html_options = @default_options.except(:skip_default_ids, :allow_method_names_outside_object)
        @multipart = nil
        @index = options[:index] || options[:child_index]
      end

      def multipart?
        @multipart
      end

      # Rails' `tag_option`: a list of values is one attribute, space-separated.
      def self.tag_options(options)
        (options || {}).to_h.transform_values { |v| v.is_a?(Array) ? v.flatten.compact.join(" ") : v }
      end

      def self.stringify(options)
        (options || {}).to_h.transform_keys(&:to_s)
      end

      def self.tag_html(name, content, options)
        n = ActionView::ViewHelpers.content_tag_name(name.to_s)
        body = ActionView::ViewHelpers.html_escape(content.to_s)
        "<#{n}#{ActionView::ViewHelpers.render_attrs(tag_options(options))}>#{ActionView::ViewHelpers.content_tag_pre(n)}#{body}</#{n}>".html_safe
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
        define_method("#{kind}_field") { |method, options = {}| tag_for_input(type, method, options) }
      end

      def hidden_field(method, options = {})
        tag_for_input("hidden", method, { "autocomplete" => "off" }.merge(FormBuilder.stringify(options)))
      end

      def text_area(method, options = {})
        opts = FormBuilder.stringify(options)
        value = opts.key?("value") ? opts.delete("value") : value_for_tag(method)
        opts["name"] ||= field_name(method)
        opts["id"] ||= field_id(method)
        wrap_field_errors(method, FormBuilder.tag_html(:textarea, value, opts))
      end
      alias textarea text_area

      def check_box(method, options = {}, checked_value = "1", unchecked_value = "0")
        opts = FormBuilder.stringify(options)
        hidden = unchecked_value.nil? ? "" : @template.tag(:input, "name" => field_name(method), "type" => "hidden", "value" => unchecked_value, "autocomplete" => "off")
        opts["type"] = "checkbox"
        opts["value"] = checked_value
        opts["name"] ||= field_name(method)
        opts["id"] ||= field_id(method)
        current = value_for_tag(method)
        opts["checked"] = "checked" if current == true || (!current.nil? && current != false && current.to_s == checked_value.to_s)
        wrap_field_errors(method, (hidden + @template.tag(:input, opts)).html_safe)
      end
      alias checkbox check_box

      # Rails' `Tags::RadioButton`: the id carries the value, sanitized.
      def radio_button(method, tag_value, options = {})
        opts = FormBuilder.stringify(options)
        opts.delete("skip_default_ids")
        opts["type"] = "radio"
        opts["value"] = tag_value
        checked = opts.key?("checked") ? opts.delete("checked") : value_for_tag(method).to_s == tag_value.to_s
        opts["checked"] = "checked" if checked == true || checked == "checked"
        opts["name"] ||= field_name(method)
        opts["id"] ||= "#{field_id(method)}_#{tag_value.to_s.gsub(/[\s.]/, "_").gsub(/[^-[[:word:]]]/, "").downcase}"
        wrap_field_errors(method, @template.tag(:input, opts))
      end

      # Rails' `Tags::Label`: the text given, else the attribute's human name.
      def label(method, text = nil, options = {}, &block)
        options, text = text, nil if text.is_a?(Hash)
        opts = FormBuilder.stringify(options)
        content = block ? @template.capture(&block) : (text || human_attribute_for_tag(method))
        opts["for"] ||= field_id(method)
        wrap_field_errors(method, FormBuilder.tag_html(:label, content, opts))
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

      def tag_for_input(type, method, options)
        opts = FormBuilder.stringify(options)
        opts["size"] = opts["maxlength"] unless opts.key?("size")
        opts["type"] ||= type
        opts["value"] = opts.key?("value") ? opts["value"] : value_for_tag(method) unless type == "password"
        opts["name"] ||= field_name(method)
        opts["id"] ||= field_id(method)
        wrap_field_errors(method, @template.tag(:input, opts))
      end

      # Rails' `value_before_type_cast`, else the reader's value.
      def value_for_tag(method)
        return nil if @object.nil?
        cast = "#{method}_before_type_cast"
        return @object.public_send(cast) if @object.respond_to?(cast)
        @object.respond_to?(method) ? @object.public_send(method) : nil
      end

      # Rails' default `field_error_proc`.
      def wrap_field_errors(method, html)
        return html unless field_errors?(method)
        @template.instance_exec(html, self, &ActionView::Base.field_error_proc)
      end

      def convert_to_model(object)
        object.respond_to?(:to_model) ? object.to_model : object
      end

      def field_errors?(method)
        return false if @object.nil?
        return @object.error_messages_for(method.to_s).any? if @object.respond_to?(:error_messages_for)
        @object.respond_to?(:errors) && @object.errors[method].any?
      end

      def human_attribute_for_tag(method)
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

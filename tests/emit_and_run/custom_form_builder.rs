use super::emit_and_run;

/// A form whose builder is the app's own subclass (`form_with(builder: ProbeFormBuilder)`): a method of that class
/// runs at request time on Rails' FormBuilder, and the stock fields it calls back (`label`, `text_field`, `text_area`,
/// `hidden_field`, `password_field`, `email_field`, `check_box`, `submit`, `button`, `@template.content_tag`,
/// `@template.safe_join`, and a safe `+` of two of them), each
/// wrapped in `field_with_errors` for an attribute with an error, answer the markup Rails 8.1.4 renders for the same
/// record. The expected strings are Rails' own output.
const BUILDER: &str = r##"class ProbeFormBuilder < ActionView::Helpers::FormBuilder
  def everything
    @template.safe_join([label(:title), text_field(:title, placeholder: "x"), text_area(:body, rows: 3), hidden_field(:title),
                         password_field(:title), email_field(:body), check_box(:title), submit, button], "|")
  end

  def boxed(attribute)
    @template.content_tag(:p, label(attribute) + text_field(attribute), class: "row")
  end
end
"##;

const FORM: &str = "<%= form_with(model: article, builder: ProbeFormBuilder) do |form| %>\n<%= form.everything %>\n<%= form.boxed :title %>\n<% end %>\n";

const SCRIPT: &str = r##"def rendered(article)
  io = +""
  Views::Articles.form_into(io, article, nil, nil)
  io
end
fresh = rendered(Article.new(title: "Hi <b>", body: "Long\nbody"))
everything = "<label for=\"article_title\">Title</label>|<input placeholder=\"x\" type=\"text\" value=\"Hi &lt;b&gt;\" name=\"article[title]\" id=\"article_title\" />|<textarea rows=\"3\" name=\"article[body]\" id=\"article_body\">\nLong\nbody</textarea>|<input autocomplete=\"off\" type=\"hidden\" value=\"Hi &lt;b&gt;\" name=\"article[title]\" id=\"article_title\" />|<input type=\"password\" name=\"article[title]\" id=\"article_title\" />|<input type=\"email\" value=\"Long\nbody\" name=\"article[body]\" id=\"article_body\" />|<input name=\"article[title]\" type=\"hidden\" value=\"0\" autocomplete=\"off\" /><input type=\"checkbox\" value=\"1\" name=\"article[title]\" id=\"article_title\" />|<input type=\"submit\" name=\"commit\" value=\"Create Article\" data-disable-with=\"Create Article\" />|<button name=\"button\" type=\"submit\">Create Article</button>"
raise "everything:\n#{fresh}" unless fresh.include?(everything)
blank = Article.new(title: "", body: "long enough body")
blank.valid?
boxed = "<p class=\"row\"><div class=\"field_with_errors\"><label for=\"article_title\">Title</label></div><div class=\"field_with_errors\"><input type=\"text\" value=\"\" name=\"article[title]\" id=\"article_title\" /></div></p>"
raise "boxed:\n#{rendered(blank)}" unless rendered(blank).include?(boxed)
puts "custom builder passed"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/helpers/probe_form_builder.rb", BUILDER).write("app/views/articles/_form.html.erb", FORM)
}

#[test]
fn a_custom_builders_own_methods_render_as_rails_on_ruby() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("custom builder passed"), "{}", run.stdout);
}

/// No other tree carries Rails' FormBuilder at request time: the builder's own method is reported, not left to
/// raise at render.
#[test]
fn a_custom_builders_own_method_is_reported_off_the_ruby_family() {
    let (_, errors) = app().emit(roundhouse::build_target::BuildTarget::Spinel);
    assert!(errors.iter().any(|e| e.contains("custom form builder method") && e.contains("form.everything")), "{errors:#?}");
}

/// maybe's `StyledFormBuilder`: a builder that defines a stock field again, by hand or by a `class_eval` loop, wraps
/// it, and Rails dispatches `form.text_field` to that override, not to the base's field.
const OVERRIDING_BUILDER: &str = r##"module PasswordWrapping
  def password_field(method, options = {})
    @template.content_tag(:div, super(method, options), class: "mixin")
  end
end

class WrappingFormBuilder < ActionView::Helpers::FormBuilder
  include PasswordWrapping

  %i[email_field text_area].each do |selector|
    class_eval <<-RUBY_EVAL, __FILE__, __LINE__ + 1
      def #{selector}(method, options = {})
        @template.content_tag(:div, super(method, options), class: "loop")
      end
    RUBY_EVAL
  end

  def text_field(method, options = {})
    @template.content_tag(:div, super(method, options.merge(class: "input")), class: "wrap")
  end
end
"##;

const OVERRIDING_FORM: &str = "<%= form_with(model: article, builder: WrappingFormBuilder) do |form| %>\n<%= form.text_field :title %>\n<%= form.email_field :body %>\n<%= form.text_area :body %>\n<%= form.hidden_field :title %>\n<%= form.password_field :title %>\n<% end %>\n";

fn overriding_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/helpers/wrapping_form_builder.rb", OVERRIDING_BUILDER)
        .write("app/views/articles/_form.html.erb", OVERRIDING_FORM)
}

#[test]
fn a_custom_builders_override_of_a_stock_field_renders_as_rails_on_ruby() {
    let run = overriding_app().run_ruby(
        r##"io = +""
Views::Articles.form_into(io, Article.new(title: "Hi", body: "Body"), nil, nil)
[
  "<div class=\"wrap\"><input class=\"input\" type=\"text\" value=\"Hi\" name=\"article[title]\" id=\"article_title\" /></div>",
  "<div class=\"loop\"><input type=\"email\" value=\"Body\" name=\"article[body]\" id=\"article_body\" /></div>",
  "<div class=\"loop\"><textarea name=\"article[body]\" id=\"article_body\">\nBody</textarea></div>",
  "<div class=\"mixin\"><input type=\"password\" name=\"article[title]\" id=\"article_title\" /></div>",
].each { |want| raise "missing #{want}:\n#{io}" unless io.include?(want) }
puts "override passed"
"##,
    );
    run.assert_passes();
    assert!(run.stdout.contains("override passed"), "{}", run.stdout);
}

#[test]
fn a_custom_builders_override_of_a_stock_field_is_reported_off_the_ruby_family() {
    let (_, errors) = overriding_app().emit(roundhouse::build_target::BuildTarget::Spinel);
    for field in ["form.text_field", "form.email_field", "form.text_area"] {
        assert!(errors.iter().any(|e| e.contains("custom form builder method") && e.contains(field)), "{field}: {errors:#?}");
    }
    assert!(!errors.iter().any(|e| e.contains("form.hidden_field")), "{errors:#?}");
}

/// maybe's `StyledFormBuilder` shape: a `class_attribute` whose default is `field_helpers` less some, and a
/// `class_eval` loop over it, which defines the stock fields again, beside `select`/`collection_select` overrides
/// calling `super`. Expected strings are Rails 8.1.4's for the same builder (Rails 8's `text_area` is an alias of the
/// base's `textarea`, so the override of `textarea` leaves it as it was).
const FIELD_HELPERS_BUILDER: &str = r##"class StyledProbeBuilder < ActionView::Helpers::FormBuilder
  class_attribute :text_field_helpers, default: field_helpers - [:label, :checkbox, :radio_button, :fields_for, :fields, :hidden_field, :file_field]

  text_field_helpers.each do |selector|
    class_eval <<-RUBY_EVAL, __FILE__, __LINE__ + 1
      def #{selector}(method, options = {})
        @template.content_tag(:div, super(method, options), class: "field")
      end
    RUBY_EVAL
  end

  def select(method, choices, options = {}, html_options = {})
    @template.content_tag(:div, super(method, choices, options, html_options.merge(class: "sel")), class: "field")
  end

  def collection_select(method, collection, value_method, text_method, options = {}, html_options = {})
    @template.content_tag(:div, super(method, collection, value_method, text_method, options, html_options), class: "field")
  end
end
"##;

const FIELD_HELPERS_FORM: &str = "<%= form_with(model: article, builder: StyledProbeBuilder) do |form| %>\n<%= form.text_field :title %>\n<%= form.textarea :body %>\n<%= form.text_area :body %>\n<%= form.select :title, [[\"Hi\", \"Hi\"], [\"Other\", \"o\"]], { include_blank: true } %>\n<%= form.collection_select :title, Article.order(:id), :title, :title, { prompt: true } %>\n<%= form.collection_select :body, Article.order(:id), :id, :title, { prompt: true } %>\n<% end %>\n";

#[test]
fn a_field_helpers_loop_builder_renders_as_rails_on_ruby() {
    let run = emit_and_run::real_blog()
        .write("app/helpers/styled_probe_builder.rb", FIELD_HELPERS_BUILDER)
        .write("app/views/articles/_form.html.erb", FIELD_HELPERS_FORM)
        .run_ruby(
            r##"Article.create!(title: "Hi", body: "A body long enough for validation.")
Article.create!(title: "Other", body: "A body long enough for validation.")
io = +""
Views::Articles.form_into(io, Article.new(title: "Hi", body: "Body"), nil, nil)
[
  "<div class=\"field\"><input type=\"text\" value=\"Hi\" name=\"article[title]\" id=\"article_title\" /></div>",
  "<div class=\"field\"><textarea name=\"article[body]\" id=\"article_body\">\nBody</textarea></div>\n<textarea name=\"article[body]\" id=\"article_body\">\nBody</textarea>",
  "<div class=\"field\"><select class=\"sel\" name=\"article[title]\" id=\"article_title\"><option value=\"\" label=\" \"></option>\n<option selected=\"selected\" value=\"Hi\">Hi</option>\n<option value=\"o\">Other</option></select></div>",
  "<div class=\"field\"><select name=\"article[title]\" id=\"article_title\"><option selected=\"selected\" value=\"Hi\">Hi</option>\n<option value=\"Other\">Other</option></select></div>",
  "<div class=\"field\"><select name=\"article[body]\" id=\"article_body\"><option value=\"1\">Hi</option>\n<option value=\"2\">Other</option></select></div>",
].each { |want| raise "missing #{want}:\n#{io}" unless io.include?(want) }
puts "field helpers passed"
"##,
        );
    run.assert_passes();
    assert!(run.stdout.contains("field helpers passed"), "{}", run.stdout);
}

/// Rails hands `fields_for` the form's `builder:`, so a nested `text_field` runs the override too, on a builder for
/// the nested name. Expected strings are Rails 8.1.4's.
#[test]
fn an_override_inside_fields_for_runs_on_the_nested_builder() {
    let form = "<%= form_with(model: article, builder: WrappingFormBuilder) do |form| %>\n<%= form.fields_for :meta, article do |nested| %>\n<%= nested.text_field :title %>\n<% end %>\n<% end %>\n";
    let run = emit_and_run::real_blog()
        .write("app/helpers/wrapping_form_builder.rb", OVERRIDING_BUILDER)
        .write("app/views/articles/_form.html.erb", form)
        .run_ruby(
            r##"io = +""
Views::Articles.form_into(io, Article.new(title: "Hi", body: "Body"), nil, nil)
[
  "<div class=\"wrap\"><input class=\"input\" type=\"text\" value=\"Hi\" name=\"article[meta][title]\" id=\"article_meta_title\" /></div>",
].each { |want| raise "missing #{want}:\n#{io}" unless io.include?(want) }
puts "nested passed"
"##,
        );
    run.assert_passes();
    assert!(run.stdout.contains("nested passed"), "{}", run.stdout);
}

/// The same builder in a Rails 7.2 app (its `Gemfile.lock`): `field_helpers` spells `text_area`/`check_box` there, so
/// the loop overrides `text_area`, which Rails 7.2 renders wrapped. Expected strings are actionview 7.2.3.1's.
#[test]
fn a_field_helpers_loop_follows_the_apps_rails_version() {
    let run = emit_and_run::real_blog()
        .edit("Gemfile.lock", "    actionview (8.1.4)\n", "    actionview (7.2.3.1)\n")
        .edit("Gemfile.lock", "    rails (8.1.4)\n", "    rails (7.2.3.1)\n")
        .write("app/helpers/styled_probe_builder.rb", &FIELD_HELPERS_BUILDER.replace(":checkbox, :radio_button", ":check_box, :radio_button"))
        .write("app/views/articles/_form.html.erb", "<%= form_with(model: article, builder: StyledProbeBuilder) do |form| %>\n<%= form.text_area :body %>\n<% end %>\n")
        .run_ruby(
            r##"io = +""
Views::Articles.form_into(io, Article.new(title: "Hi", body: "Body"), nil, nil)
want = "<div class=\"field\"><textarea name=\"article[body]\" id=\"article_body\">\nBody</textarea></div>"
raise "missing #{want}:\n#{io}" unless io.include?(want)
puts "rails 7 passed"
"##,
        );
    run.assert_passes();
    assert!(run.stdout.contains("rails 7 passed"), "{}", run.stdout);
}

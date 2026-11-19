use super::emit_and_run;

/// A form whose builder is the app's own subclass (`form_with(builder: ProbeFormBuilder)`): a method of that class
/// runs at request time on Rails' FormBuilder, and the stock fields it calls back (`label`, `text_field`, `text_area`,
/// `hidden_field`, `password_field`, `email_field`, `check_box`, `submit`, `button`, `@template.content_tag`,
/// `@template.safe_join`, and a safe `+` of two of them), each
/// wrapped in `field_with_errors` for an attribute with an error, answer the markup Rails 8.1.4 renders for the same
/// record. The expected strings are Rails' own output under the blog's `load_defaults` 8.1 (no hidden `autocomplete`).
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
everything = "<label for=\"article_title\">Title</label>|<input placeholder=\"x\" type=\"text\" value=\"Hi &lt;b&gt;\" name=\"article[title]\" id=\"article_title\" />|<textarea rows=\"3\" name=\"article[body]\" id=\"article_body\">\nLong\nbody</textarea>|<input type=\"hidden\" value=\"Hi &lt;b&gt;\" name=\"article[title]\" id=\"article_title\" />|<input type=\"password\" name=\"article[title]\" id=\"article_title\" />|<input type=\"email\" value=\"Long\nbody\" name=\"article[body]\" id=\"article_body\" />|<input name=\"article[title]\" type=\"hidden\" value=\"0\" /><input type=\"checkbox\" value=\"1\" name=\"article[title]\" id=\"article_title\" />|<input type=\"submit\" name=\"commit\" value=\"Create Article\" data-disable-with=\"Create Article\" />|<button name=\"button\" type=\"submit\">Create Article</button>"
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

/// A partial handed the builder alone (`render "articles/fields", f: form`) reads the form's record through it, and a
/// custom builder's override runs there as in the form: Rails 8.1.4's markup, compared as attribute sets where the
/// stock field is written inline (attribute order is not markup, `docs/pipeline/verification.md`).
const ATTRS: &str = r##"def attrs(html, name)
  tag = html[/<input [^>]*name="#{Regexp.escape(name)}"[^>]*>/] or raise "no input #{name}:\n#{html}"
  tag.scan(/(\S+?)="([^"]*)"/).to_h
end
"##;

fn partial_app(form: &str) -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/helpers/wrapping_form_builder.rb", OVERRIDING_BUILDER)
        .write("app/views/articles/_form.html.erb", form)
        .write("app/views/articles/_fields.html.erb", "<%= f.text_field :title %>\n<%= f.text_area :body %>\n")
}

#[test]
fn a_partial_handed_the_builder_alone_renders_its_fields() {
    let run = partial_app("<%= form_with(model: article) do |form| %>\n<%= render \"articles/fields\", f: form %>\n<% end %>\n").run_ruby(&format!(
        r##"{ATTRS}io = +""
Views::Articles.form_into(io, Article.new(title: "Hi", body: "Body"), nil, nil)
want = {{ "type" => "text", "value" => "Hi", "name" => "article[title]", "id" => "article_title" }}
raise "title #{{attrs(io, "article[title]")}}" unless attrs(io, "article[title]") == want
raise "body:\n#{{io}}" unless io.include?("id=\"article_body\">\nBody</textarea>")
puts "partial passed"
"##
    ));
    run.assert_passes();
    assert!(run.stdout.contains("partial passed"), "{}", run.stdout);
}

#[test]
fn a_partial_of_a_custom_builder_runs_its_overrides() {
    let run = partial_app("<%= form_with(model: article, builder: ::WrappingFormBuilder) do |form| %>\n<%= render \"articles/fields\", f: form %>\n<% end %>\n").run_ruby(
        r##"io = +""
Views::Articles.form_into(io, Article.new(title: "Hi", body: "Body"), nil, nil)
[
  "<div class=\"wrap\"><input class=\"input\" type=\"text\" value=\"Hi\" name=\"article[title]\" id=\"article_title\" /></div>",
  "<div class=\"loop\"><textarea name=\"article[body]\" id=\"article_body\">\nBody</textarea></div>",
].each { |want| raise "missing #{want}:\n#{io}" unless io.include?(want) }
puts "custom partial passed"
"##,
    );
    run.assert_passes();
    assert!(run.stdout.contains("custom partial passed"), "{}", run.stdout);
}

/// A `fields_for` inside a `fields_for` is the same builder class in Rails, though the analysis types the inner one as
/// the base: its override is reported off the Ruby family as the outer level's is.
#[test]
fn an_override_two_fields_for_deep_is_reported_off_the_ruby_family() {
    let form = "<%= form_with(model: article, builder: WrappingFormBuilder) do |form| %>\n<%= form.fields_for :meta, article do |a| %>\n<%= a.fields_for :inner, article do |b| %>\n<%= b.text_field :title %>\n<% end %>\n<% end %>\n<% end %>\n";
    let (_, errors) = emit_and_run::real_blog()
        .write("app/helpers/wrapping_form_builder.rb", OVERRIDING_BUILDER)
        .write("app/views/articles/_form.html.erb", form)
        .emit(roundhouse::build_target::BuildTarget::Spinel);
    assert!(errors.iter().any(|e| e.contains("custom form builder method") && e.contains("text_field")), "{errors:#?}");
}

/// The partial's prefix is the record's `param_key`, not the local holding it (`edited` → `article[title]`), and a
/// block that rebinds the builder's name inside the form hands its own value, not the builder.
#[test]
fn a_partial_handed_the_builder_alone_names_fields_by_the_model() {
    let form = "<% edited = article %>\n<%= form_with(model: edited) do |form| %>\n<%= render \"articles/fields\", f: form %>\n<% [\"x\"].each do |form| %><%= render \"articles/row\", value: form %><% end %>\n<% end %>\n";
    let run = partial_app(form)
        .write("app/views/articles/_row.html.erb", "<p><%= value %></p>\n")
        .run_ruby(&format!(
            r##"{ATTRS}io = +""
Views::Articles.form_into(io, Article.new(title: "Hi", body: "Body"), nil, nil)
raise "title #{{attrs(io, "article[title]")}}" unless attrs(io, "article[title]")["id"] == "article_title"
raise "row:\n#{{io}}" unless io.include?("<p>x</p>")
puts "named passed"
"##
        ));
    run.assert_passes();
    assert!(run.stdout.contains("named passed"), "{}", run.stdout);
}

/// A strict-locals partial (`locals: (f:, caption:)`) handed the builder alone takes the record first, as its lowering
/// reads one, beside the locals it declares.
#[test]
fn a_strict_locals_partial_handed_the_builder_alone_renders_its_fields() {
    let run = emit_and_run::real_blog()
        .write("app/views/articles/_form.html.erb", "<%= form_with(model: article) do |form| %>\n<%= render \"articles/fields\", f: form, caption: \"Cap\" %>\n<% end %>\n")
        .write("app/views/articles/_fields.html.erb", "<%# locals: (f:, caption: \"none\") -%>\n<p><%= caption %></p>\n<%= f.text_field :title %>\n")
        .run_ruby(&format!(
            r##"{ATTRS}io = +""
Views::Articles.form_into(io, Article.new(title: "Hi", body: "Body"), nil, nil)
raise "title #{{attrs(io, "article[title]")}}" unless attrs(io, "article[title]")["value"] == "Hi"
raise "caption:\n#{{io}}" unless io.include?("<p>Cap</p>")
puts "strict passed"
"##
        ));
    run.assert_passes();
    assert!(run.stdout.contains("strict passed"), "{}", run.stdout);
}

/// A `fields_for` builder handed to a partial (forem's `render "poll_fields", f: poll_form`): Rails hands it the
/// nested builder object, which the Ruby family builds from its overlay; other targets report it. Rails 8.1.4's names.
fn nested_partial_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/views/articles/_form.html.erb", "<%= form_with(model: article) do |form| %>\n<%= form.fields_for :meta, article do |child| %>\n<%= render \"articles/fields\", f: child %>\n<% end %>\n<% end %>\n")
        .write("app/views/articles/_fields.html.erb", "<%= f.text_field :title %>\n")
}

#[test]
fn a_fields_for_builder_handed_to_a_partial_renders_on_ruby() {
    let run = nested_partial_app().run_ruby(
        r##"io = +""
Views::Articles.form_into(io, Article.new(title: "Hi", body: "Body"), nil, nil)
want = "<input type=\"text\" value=\"Hi\" name=\"article[meta][title]\" id=\"article_meta_title\" />"
raise "missing #{want}:\n#{io}" unless io.include?(want)
puts "nested partial passed"
"##,
    );
    run.assert_passes();
    assert!(run.stdout.contains("nested partial passed"), "{}", run.stdout);
}

#[test]
fn a_fields_for_builder_handed_to_a_partial_is_reported_off_the_ruby_family() {
    let (_, errors) = nested_partial_app().emit(roundhouse::build_target::BuildTarget::Spinel);
    assert!(errors.iter().any(|e| e.contains("custom form builder method") && e.contains("fields_for")), "{errors:#?}");
}

/// Rails hands the nested builder the form's `namespace:`, so an override's field ids carry it.
#[test]
fn a_nested_builder_keeps_the_forms_namespace() {
    let form = "<%= form_with(model: article, builder: WrappingFormBuilder, namespace: \"editor\") do |form| %>\n<%= form.fields_for :meta, article do |nested| %>\n<%= nested.text_field :title %>\n<% end %>\n<% end %>\n";
    let run = emit_and_run::real_blog()
        .write("app/helpers/wrapping_form_builder.rb", OVERRIDING_BUILDER)
        .write("app/views/articles/_form.html.erb", form)
        .run_ruby(
            r##"io = +""
Views::Articles.form_into(io, Article.new(title: "Hi", body: "Body"), nil, nil)
raise "namespace:\n#{io}" unless io.include?("id=\"editor_article_meta_title\"")
puts "namespace passed"
"##,
        );
    run.assert_passes();
    assert!(run.stdout.contains("namespace passed"), "{}", run.stdout);
}

/// `fields_for :comments` over a has_many's nested attributes: Rails 8.1.4 writes each child under
/// `article[comments_attributes][i]`, counting `i` from 0, with the persisted child's hidden id after its fields, and
/// an explicit `child_index:` for one record given. Attribute sets, since order is not markup.
#[test]
fn fields_for_over_a_has_many_nested_attribute_names_as_rails() {
    let run = nested_many_app().run_ruby(&nested_many_script());
    run.assert_passes();
    assert!(run.stdout.contains("has_many passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn fields_for_over_a_has_many_nested_attribute_names_as_rails_on_spinel() {
    let run = nested_many_app().run_spinel_with_db(&nested_many_script());
    run.assert_passes();
    assert!(run.stdout.contains("has_many passed"), "{}", run.stdout);
}

fn nested_many_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  accepts_nested_attributes_for :comments\n")
        .write("app/views/articles/_form.html.erb", "<%= form_with(model: article) do |form| %>\n<%= form.fields_for :comments do |c| %>\n<%= c.text_field :body %>\n<% end %>\n<%= form.fields_for :comments, Comment.new(body: \"New\"), child_index: \"IDX\" do |c| %>\n<%= c.text_field :body %>\n<% end %>\n<% end %>\n")
}

fn nested_many_script() -> String {
    format!(
            r##"{ATTRS}a = Article.create!(title: "Hi", body: "A body long enough for validation.")
one = Comment.create!(article_id: a.id, commenter: "me", body: "One")
two = Comment.create!(article_id: a.id, commenter: "me", body: "Two")
io = +""
Views::Articles.form_into(io, Article.find(a.id), nil, nil)
{{
  "article[comments_attributes][0][body]" => {{ "type" => "text", "value" => "One", "id" => "article_comments_attributes_0_body" }},
  "article[comments_attributes][0][id]" => {{ "type" => "hidden", "value" => one.id.to_s, "id" => "article_comments_attributes_0_id" }},
  "article[comments_attributes][1][body]" => {{ "type" => "text", "value" => "Two", "id" => "article_comments_attributes_1_body" }},
  "article[comments_attributes][1][id]" => {{ "type" => "hidden", "value" => two.id.to_s, "id" => "article_comments_attributes_1_id" }},
  "article[comments_attributes][IDX][body]" => {{ "type" => "text", "value" => "New", "id" => "article_comments_attributes_IDX_body" }},
}}.each do |name, want|
  got = attrs(io, name)
  raise "#{{name}}: #{{got}}" unless got == want.merge("name" => name)
end
raise "new record has no id:\n#{{io}}" if io.include?("article[comments_attributes][IDX][id]")
puts "has_many passed"
"##
    )
}

/// `fields_for :article` over a belongs_to the model accepts nested attributes for: Rails 8.1.4 names the fields
/// `comment[article_attributes][…]` and adds the persisted record's hidden id after them.
#[test]
fn fields_for_over_a_singular_nested_attribute_names_as_rails() {
    let run = emit_and_run::real_blog()
        .edit("app/models/comment.rb", "  belongs_to :article\n", "  belongs_to :article\n  accepts_nested_attributes_for :article\n")
        .write("app/views/comments/_comment.html.erb", "<%= form_with(model: comment, url: \"/c\") do |form| %>\n<%= form.fields_for :article do |a| %>\n<%= a.text_field :title %>\n<% end %>\n<% end %>\n")
        .run_ruby(&format!(
            r##"{ATTRS}a = Article.create!(title: "Hi", body: "A body long enough for validation.")
c = Comment.create!(article_id: a.id, commenter: "me", body: "A comment body.")
io = +""
Views::Comments.comment_into(io, c, nil, nil)
raise "title #{{attrs(io, "comment[article_attributes][title]")}}" unless attrs(io, "comment[article_attributes][title]") == {{ "type" => "text", "value" => "Hi", "name" => "comment[article_attributes][title]", "id" => "comment_article_attributes_title" }}
raise "id #{{attrs(io, "comment[article_attributes][id]")}}" unless attrs(io, "comment[article_attributes][id]") == {{ "type" => "hidden", "value" => a.id.to_s, "name" => "comment[article_attributes][id]", "id" => "comment_article_attributes_id" }}
puts "nested attributes passed"
"##
        ));
    run.assert_passes();
    assert!(run.stdout.contains("nested attributes passed"), "{}", run.stdout);
}

/// `form_with(model: comment)` for a local the view assigns, under `articles/`: Rails names the fields after the
/// record's `param_key` (`comment[body]`), not the directory.
#[test]
fn a_form_for_a_view_local_names_fields_by_its_model() {
    let run = emit_and_run::real_blog()
        .write("app/views/articles/_form.html.erb", "<% comment = article.comments.first %>\n<% if comment %><%= form_with(model: comment, url: \"/c\") do |form| %>\n<%= form.text_field :body %>\n<% end %><% end %>\n")
        .run_ruby(&format!(
            r##"{ATTRS}a = Article.create!(title: "Hi", body: "A body long enough for validation.")
Comment.create!(article_id: a.id, commenter: "me", body: "A comment body.")
io = +""
Views::Articles.form_into(io, a, nil, nil)
raise "body #{{attrs(io, "comment[body]")}}" unless attrs(io, "comment[body]")["id"] == "comment_body"
puts "local passed"
"##
        ));
    run.assert_passes();
    assert!(run.stdout.contains("local passed"), "{}", run.stdout);
}

/// A body that writes the record's hidden id only under a condition: Rails adds its own unless that write ran, which
/// the lowering would decide at compile time; reported rather than drop the id when the condition is false.
#[test]
fn a_conditional_hidden_id_in_nested_attributes_is_reported() {
    let (_, errors) = emit_and_run::real_blog()
        .edit("app/models/comment.rb", "  belongs_to :article\n", "  belongs_to :article\n  accepts_nested_attributes_for :article\n")
        .write("app/views/comments/_comment.html.erb", "<%= form_with(model: comment, url: \"/c\") do |form| %>\n<%= form.fields_for :article do |a| %>\n<% if comment.body.present? %><%= a.hidden_field :id %><% end %>\n<% end %>\n<% end %>\n")
        .emit(roundhouse::build_target::BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("hidden id only under a condition")), "{errors:#?}");
}

/// A partial of an overriding builder takes the caller's builder object, as Rails hands it: an override that keeps
/// state (`@sequence`) counts on across the partial boundary.
#[test]
fn a_partial_of_a_custom_builder_shares_its_object() {
    let builder = "class CountingFormBuilder < ActionView::Helpers::FormBuilder\n  def text_field(method, options = {})\n    @sequence = (@sequence || 0) + 1\n    super(method, options.merge(name: \"answers[#{@sequence}]\"))\n  end\nend\n";
    let run = emit_and_run::real_blog()
        .write("app/helpers/counting_form_builder.rb", builder)
        .write("app/views/articles/_form.html.erb", "<%= form_with(model: article, builder: CountingFormBuilder) do |form| %>\n<%= form.text_field :title %>\n<%= render \"articles/fields\", f: form %>\n<% end %>\n")
        .write("app/views/articles/_fields.html.erb", "<%= f.text_field :body %>\n")
        .run_ruby(
            r##"io = +""
Views::Articles.form_into(io, Article.new(title: "Hi", body: "Body"), nil, nil)
raise "sequence:\n#{io}" unless io.include?("name=\"answers[1]\"") && io.include?("name=\"answers[2]\"")
puts "shared passed"
"##,
        );
    run.assert_passes();
    assert!(run.stdout.contains("shared passed"), "{}", run.stdout);
}

/// `form_with(model: [article, comment])` names its fields after the last element (`comment[body]`), and a partial
/// handed its builder alone binds to that record too.
#[test]
fn a_partial_of_an_array_model_form_binds_its_last_record() {
    let run = emit_and_run::real_blog()
        .write("app/views/articles/_form.html.erb", "<% comment = Comment.new(body: \"Hi\") %>\n<%= form_with(model: [article, comment]) do |form| %>\n<%= render \"articles/fields\", f: form %>\n<% end %>\n")
        .write("app/views/articles/_fields.html.erb", "<%= f.text_field :body %>\n")
        .run_ruby(&format!(
            r##"{ATTRS}a = Article.create!(title: "Hi", body: "A body long enough for validation.")
io = +""
Views::Articles.form_into(io, a, nil, nil)
raise "body #{{attrs(io, "comment[body]")}}" unless attrs(io, "comment[body]")["value"] == "Hi"
puts "array passed"
"##
        ));
    run.assert_passes();
    assert!(run.stdout.contains("array passed"), "{}", run.stdout);
}

/// maybe's rules form: a custom builder's `fields_for` over a has_many's nested attributes, each child's builder
/// handed to a partial, whose override runs on it under the child's index.
#[test]
fn a_custom_builders_nested_children_render_in_a_partial() {
    let run = emit_and_run::real_blog()
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  accepts_nested_attributes_for :comments\n")
        .write("app/helpers/wrapping_form_builder.rb", OVERRIDING_BUILDER)
        .write("app/views/articles/_form.html.erb", "<%= form_with(model: article, builder: WrappingFormBuilder) do |form| %>\n<%= form.fields_for :comments do |c| %>\n<%= render \"articles/fields\", f: c %>\n<% end %>\n<% end %>\n")
        .write("app/views/articles/_fields.html.erb", "<%= f.text_field :body %>\n")
        .run_ruby(
            r##"a = Article.create!(title: "Hi", body: "A body long enough for validation.")
Comment.create!(article_id: a.id, commenter: "me", body: "One")
Comment.create!(article_id: a.id, commenter: "me", body: "Two")
io = +""
Views::Articles.form_into(io, Article.find(a.id), nil, nil)
[0, 1].zip(%w[One Two]).each do |i, body|
  want = "<div class=\"wrap\"><input class=\"input\" type=\"text\" value=\"#{body}\" name=\"article[comments_attributes][#{i}][body]\" id=\"article_comments_attributes_#{i}_body\" /></div>"
  raise "missing #{want}:\n#{io}" unless io.include?(want)
end
puts "children passed"
"##,
        );
    run.assert_passes();
    assert!(run.stdout.contains("children passed"), "{}", run.stdout);
}

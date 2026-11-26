//! A form builder's fields in the attribute order Rails 8.1.4 writes them, closed by `tag`'s ` />`: the caller's
//! options first, then `type`, the value, `name` and `id` (`Tags::TextField#render`), with each field's own twist —
//! a password's `value:` leads, a color's value comes before its `type`, a
//! checkbox and radio put `checked` after the value, a textarea's `size` splits after its `id`, and a submit's
//! options come before `data-disable-with`. The fields used to open with `type`, `name` and `id` and close with `>`.

use super::emit_and_run;

#[path = "../support/prerequisite.rs"]
mod prerequisite;

const SCHEMA: &str = "ActiveRecord::Schema.define do\ncreate_table(:entries) { |t| t.string :title; t.string :color; t.string :kind; t.integer :count; t.boolean :enabled }\nend\n";

const MODEL: &str = "class Entry < ActiveRecord::Base\nend\n";

const FIELDS: &str = r#"<%= form_with model: @entry, url: "/" do |form| %>
<%= form.label :title, class: "l" %>
<%= form.text_field :title, class: "c" %>
<%= form.text_field :title, value: "V", maxlength: 9 %>
<%= form.email_field :title, placeholder: "p" %>
<%= form.number_field :count %>
<%= form.password_field :title, class: "c", value: "s" %>
<%= form.hidden_field :title, class: "c" %>
<%= form.color_field :color %>
<%= form.radio_button :kind, "a" %>
<%= form.check_box :enabled, class: "c" %>
<%= form.text_area :title, size: "5x2", class: "c" %>
<%= form.submit "Go", class: "b" %>
<% end %>"#;

const EXPECTED: &str = r##"<label class="l" for="entry_title">Title</label>
<input class="c" type="text" value="Hi" name="entry[title]" id="entry_title" />
<input value="V" maxlength="9" size="9" type="text" name="entry[title]" id="entry_title" />
<input placeholder="p" type="email" value="Hi" name="entry[title]" id="entry_title" />
<input type="number" value="2" name="entry[count]" id="entry_count" />
<input value="s" class="c" type="password" name="entry[title]" id="entry_title" />
<input class="c" type="hidden" value="Hi" name="entry[title]" id="entry_title" />
<input value="#000000" type="color" name="entry[color]" id="entry_color" />
<input type="radio" value="a" checked="checked" name="entry[kind]" id="entry_kind_a" />
<input name="entry[enabled]" type="hidden" value="0" /><input class="c" type="checkbox" value="1" checked="checked" name="entry[enabled]" id="entry_enabled" />
<textarea class="c" name="entry[title]" id="entry_title" cols="5" rows="2">
Hi</textarea>
<input type="submit" name="commit" value="Go" class="b" data-disable-with="Go" />
"##;

const RECORD: &str = r#"Entry.new(title: "Hi", kind: "a", count: 2, enabled: true)"#;

/// The rendered form between its opening tag's hidden inputs and `</form>`.
const FIELDS_OF: &str = r#"def fields_of(html)
  html[html.index("<label")...html.index("</form>")]
end
"#;

fn form_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", SCHEMA)
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write("app/models/entry.rb", MODEL)
        .write("app/controllers/pages_controller.rb", &format!("class PagesController < ActionController::Base\n  def form\n    @entry = {RECORD}\n  end\nend\n"))
        .write("app/views/pages/form.html.erb", FIELDS)
}

fn form_script() -> String {
    format!(
        r##"{FIELDS_OF}fields = fields_of(Views::Pages.form({RECORD}))
raise "fields: #{{fields.inspect}}" unless fields == {EXPECTED:?}
puts "form field order passed"
"##
    )
}

#[test]
fn form_fields_write_rails_attribute_order() {
    form_app().run_ruby(&form_script()).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn form_fields_write_rails_attribute_order_on_spinel() {
    form_app().run_spinel_with_db(&form_script()).assert_passes();
}

#[test]
fn form_field_order_matches_rails() {
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/rails_oracle.rb");
    let script = format!(
        r##"require {helper:?}
unless RailsOracle.available?("activerecord", "actionview", "actionpack")
  warn "skipping form field oracle: #{{RailsOracle.label}} needs activerecord, actionview and actionpack"
  exit 77
end
RailsOracle.activate("activerecord", "actionview", "actionpack")
require "active_record"
require "action_view"
require "action_controller"
ActiveRecord::Base.logger = nil
ActiveRecord::Migration.verbose = false
ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
{SCHEMA}{MODEL}ActionView::Helpers::FormHelper.form_with_generates_ids = true
ActionView::Base.remove_hidden_field_autocomplete = true
view = ActionView::Base.with_empty_template_cache.new(ActionView::LookupContext.new([]), {{}}, nil)
view.define_singleton_method(:protect_against_forgery?) {{ false }}
view.assign(entry: {RECORD})
{FIELDS_OF}print fields_of(view.render(inline: {FIELDS:?}))
"##
    );
    let output = std::process::Command::new("ruby").args(["-e", &script]).output().expect("Rails oracle");
    if output.status.code() == Some(77) {
        prerequisite::missing(String::from_utf8_lossy(&output.stderr).trim());
        return;
    }
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED);
}

/// Before `load_defaults 8.1`, a hidden field carries `autocomplete="off"`, and `HiddenField` reverse-merges it, so
/// it leads the options even when the caller set it.
const HIDDEN: &str = r#"<%= form_with model: @entry, url: "/" do |form| %>
<label></label><%= form.hidden_field :title, class: "c" %>
<%= form.hidden_field :title, class: "c", autocomplete: "on" %>
<%= form.check_box :enabled %>
<% end %>"#;

const HIDDEN_EXPECTED: &str = r##"<label></label><input autocomplete="off" class="c" type="hidden" value="Hi" name="entry[title]" id="entry_title" />
<input autocomplete="on" class="c" type="hidden" value="Hi" name="entry[title]" id="entry_title" />
<input name="entry[enabled]" type="hidden" value="0" autocomplete="off" /><input type="checkbox" value="1" checked="checked" name="entry[enabled]" id="entry_enabled" />
"##;

const APPLICATION: &str = "require_relative \"boot\"\nrequire \"rails/all\"\nmodule Probe\n  class Application < Rails::Application\n    config.load_defaults 8.0\n  end\nend\n";

#[test]
fn hidden_fields_lead_with_autocomplete_before_rails_8_1() {
    emit_and_run::empty_app()
        .write("db/schema.rb", SCHEMA)
        .write("config/application.rb", APPLICATION)
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write("app/models/entry.rb", MODEL)
        .write("app/controllers/pages_controller.rb", &format!("class PagesController < ActionController::Base\n  def hidden\n    @entry = {RECORD}\n  end\nend\n"))
        .write("app/views/pages/hidden.html.erb", HIDDEN)
        .run_ruby(&format!(
            r##"{FIELDS_OF}fields = fields_of(Views::Pages.hidden({RECORD}))
raise "fields: #{{fields.inspect}}" unless fields == {HIDDEN_EXPECTED:?}
puts "hidden field order passed"
"##
        ))
        .assert_passes();
}

#[test]
fn hidden_field_order_matches_rails() {
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/rails_oracle.rb");
    let script = format!(
        r##"require {helper:?}
unless RailsOracle.available?("activerecord", "actionview", "actionpack")
  warn "skipping hidden field oracle: #{{RailsOracle.label}} needs activerecord, actionview and actionpack"
  exit 77
end
RailsOracle.activate("activerecord", "actionview", "actionpack")
require "active_record"
require "action_view"
require "action_controller"
ActiveRecord::Base.logger = nil
ActiveRecord::Migration.verbose = false
ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
{SCHEMA}{MODEL}ActionView::Helpers::FormHelper.form_with_generates_ids = true
ActionView::Base.remove_hidden_field_autocomplete = false
view = ActionView::Base.with_empty_template_cache.new(ActionView::LookupContext.new([]), {{}}, nil)
view.define_singleton_method(:protect_against_forgery?) {{ false }}
view.assign(entry: {RECORD})
{FIELDS_OF}print fields_of(view.render(inline: {HIDDEN:?}))
"##
    );
    let output = std::process::Command::new("ruby").args(["-e", &script]).output().expect("Rails oracle");
    if output.status.code() == Some(77) {
        prerequisite::missing(String::from_utf8_lossy(&output.stderr).trim());
        return;
    }
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), HIDDEN_EXPECTED);
}

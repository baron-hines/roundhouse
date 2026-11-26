//! Rails writes `autocomplete="off"` on every hidden input until
//! `config.load_defaults` 8.1 turns `remove_hidden_field_autocomplete` on:
//! maybe (7.2) renders it on a form's CSRF token, a `hidden_field`, a
//! `check_box`'s unchecked value and a `button_to`'s `_method` and token, and the
//! blog (8.1) on none. Attribute sets, since order is not markup.

use super::emit_and_run;

const FORM: &str = "<%= form_with(model: article) do |form| %>\n<%= form.hidden_field :title %>\n<%= form.check_box :title %>\n<% end %>\n<%= button_to \"Delete\", \"/articles/1\", method: :delete %>\n";

const SCRIPT: &str = r##"io = +""
Views::Articles.form_into(io, Article.new(title: "Hi", body: "Body"), nil, nil)
hidden = io.scan(/<input [^>]*type="hidden"[^>]*>/)
raise "hidden inputs:\n#{io}" unless hidden.size == 5
marked = hidden.count { |tag| tag.include?('autocomplete="off"') }
puts "hidden #{hidden.size} marked #{marked}"
"##;

fn app(load_defaults: &str) -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("config/application.rb", "config.load_defaults 8.1", &format!("config.load_defaults {load_defaults}"))
        .write("app/views/articles/_form.html.erb", FORM)
}

#[test]
fn hidden_inputs_carry_autocomplete_before_load_defaults_8_1() {
    let run = app("7.2").run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("hidden 5 marked 5"), "{}", run.stdout);
}

#[test]
fn hidden_inputs_drop_autocomplete_under_load_defaults_8_1() {
    let run = app("8.1").run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("hidden 5 marked 0"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn hidden_inputs_carry_autocomplete_before_load_defaults_8_1_on_spinel() {
    let run = app("7.2").run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("hidden 5 marked 5"), "{}", run.stdout);
}

/// A setting under a modifier (`= false if false`) is not taken as set: 8.1's default stays.
#[test]
fn a_conditional_setting_leaves_the_default() {
    let run = app("8.1")
        .write("config/initializers/hidden_autocomplete.rb", "Rails.application.config.action_view.remove_hidden_field_autocomplete = false if false\n")
        .run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("hidden 5 marked 0"), "{}", run.stdout);
}

/// A plain setting ended by `;` is set.
#[test]
fn a_setting_ended_by_a_semicolon_is_set() {
    let run = app("8.1")
        .write("config/initializers/hidden_autocomplete.rb", "Rails.application.config.action_view.remove_hidden_field_autocomplete = false;\n")
        .run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("hidden 5 marked 5"), "{}", run.stdout);
}

//! A model's own `<column>?` is the one Rails calls. ActiveRecord defines
//! attribute methods in a generated module the model includes, so a
//! `def done?` in the class body overrides the attribute query for a
//! `done` column. The lowering used to keep its generated predicate and
//! drop the model's method as a duplicate (unless the name was also an
//! enum label), so the app answered the column's value instead.
//!
//! A `def` that calls `super` wants the generated predicate, which has no
//! other name to reach it by, so that case keeps the generated one.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;

const SCHEMA: &str = r#"ActiveRecord::Schema[8.1].define(version: 1) do
  create_table "widgets", force: :cascade do |t|
    t.string "name"
    t.boolean "done", default: false, null: false
  end
end
"#;

fn app(widget: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", SCHEMA)
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n")
        .write("app/models/widget.rb", widget)
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::API\nend\n")
        .write(
            "app/controllers/widgets_controller.rb",
            "class WidgetsController < ApplicationController\n  def show\n    render json: { done: Widget.find(params[:id]).done? }\n  end\nend\n",
        )
        .write("config/routes.rb", "Rails.application.routes.draw do\n  resources :widgets, only: %i[show]\nend\n")
}

const OWN: &str = "class Widget < ApplicationRecord\n  def done? = name.present? || done\nend\n";

const TWICE: &str = "class Widget < ApplicationRecord\n  def done? = name.present? || done\n\n  def done? = name.present? || done\nend\n";

const CALLS_SUPER: &str = "class Widget < ApplicationRecord\n  def done?\n    super && name.present?\n  end\nend\n";

const CHECK: &str = r##"
named = Widget.create!(name: "w", done: false)
raise "named, not done: #{named.done?.inspect}" unless named.done? == true
blank = Widget.create!(name: "", done: false)
raise "blank, not done: #{blank.done?.inspect}" unless blank.done? == false
finished = Widget.create!(name: "", done: true)
raise "blank, done: #{finished.done?.inspect}" unless finished.done? == true
status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/widgets/#{named.id}", "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
text = +""
body.each { |part| text << part }
raise "#{status} #{text}" unless status == 200 && text == %({"done":true})
puts "ALL OK"
"##;

/// The emitted model's `done?` definitions, one string each.
fn done_predicates(widget: &str, target: BuildTarget) -> Vec<String> {
    let (emitted, errors) = app(widget).emit(target);
    assert!(errors.is_empty(), "{errors:?}");
    let model = std::fs::read_to_string(emitted.join("app/models/widget.rb")).unwrap();
    let lines: Vec<&str> = model.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim_start().starts_with("def done?"))
        .map(|(i, l)| {
            if l.contains('=') {
                return l.trim().to_string();
            }
            let indent = l.len() - l.trim_start().len();
            let end = lines[i + 1..]
                .iter()
                .position(|e| e.len() - e.trim_start().len() == indent && e.trim() == "end")
                .map_or(lines.len(), |p| i + 1 + p);
            lines[i..=end.min(lines.len() - 1)].join("\n")
        })
        .collect()
}

fn assert_own_body(widget: &str, target: BuildTarget) {
    let defs = done_predicates(widget, target);
    assert_eq!(defs.len(), 1, "{target:?}: {defs:#?}");
    assert!(defs[0].contains("present?"), "{target:?}: the model's body was dropped:\n{}", defs[0]);
}

#[test]
fn a_models_own_column_predicate_replaces_the_generated_one() {
    assert_own_body(OWN, BuildTarget::Ruby);
    assert_own_body(OWN, BuildTarget::Spinel);
}

#[test]
fn a_models_own_column_predicate_runs() {
    app(OWN).run_ruby(CHECK).assert_passes();
}

#[test]
fn a_column_predicate_defined_twice_keeps_the_models_body() {
    assert_own_body(TWICE, BuildTarget::Ruby);
    assert_own_body(TWICE, BuildTarget::Spinel);
    app(TWICE).run_ruby(CHECK).assert_passes();
}

#[test]
fn a_column_predicate_calling_super_keeps_the_generated_one() {
    for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
        let defs = done_predicates(CALLS_SUPER, target);
        assert_eq!(defs.len(), 1, "{target:?}: {defs:#?}");
        assert!(!defs[0].contains("present?"), "{target:?}: {}", defs[0]);
        assert!(defs[0].contains("@done"), "{target:?}: {}", defs[0]);
    }
}

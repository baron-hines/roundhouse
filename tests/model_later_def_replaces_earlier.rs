//! A model's class body runs top to bottom, so a second `def` of a name
//! replaces the first and is what every call reaches (Ruby warns
//! "method redefined; discarding old"). The lowering used to keep the
//! first `def` and drop the later one, for instance methods, `def self.`
//! methods and a later `def` with another arity.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;

const SCHEMA: &str = r#"ActiveRecord::Schema[8.1].define(version: 1) do
  create_table "widgets", force: :cascade do |t|
    t.string "name", null: false
  end
end
"#;

const WIDGET: &str = r#"class Widget < ApplicationRecord
  def self.flavor = "class-first"
  def self.flavor = "class-second"

  def summary
    "first"
  end

  def summary
    "second"
  end

  def size(a)
    "one-arg"
  end

  def size(a, b = nil)
    "two-arg"
  end
end
"#;

fn app(widget: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", SCHEMA)
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n")
        .write("app/models/widget.rb", widget)
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::API\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
}

fn emitted_model(widget: &str, target: BuildTarget) -> String {
    let (emitted, errors) = app(widget).emit(target);
    assert!(errors.is_empty(), "{errors:?}");
    std::fs::read_to_string(emitted.join("app/models/widget.rb")).unwrap()
}

#[test]
fn a_later_instance_def_is_the_one_emitted() {
    for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
        let model = emitted_model(WIDGET, target);
        assert_eq!(model.matches("def summary").count(), 1, "{target:?}:\n{model}");
        assert!(model.contains("\"second\"") && !model.contains("\"first\""), "{target:?}:\n{model}");
    }
}

#[test]
fn a_later_class_def_is_the_one_emitted() {
    for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
        let model = emitted_model(WIDGET, target);
        assert_eq!(model.matches("def self.flavor").count(), 1, "{target:?}:\n{model}");
        assert!(model.contains("class-second") && !model.contains("class-first"), "{target:?}:\n{model}");
    }
}

#[test]
fn a_later_def_with_another_arity_is_the_one_emitted() {
    for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
        let model = emitted_model(WIDGET, target);
        assert!(model.contains("\"two-arg\"") && !model.contains("\"one-arg\""), "{target:?}:\n{model}");
    }
    let (emitted, errors) = app(WIDGET).emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    let rbs = std::fs::read_to_string(emitted.join("app/models/widget.rbs")).unwrap();
    assert!(rbs.contains("def size: (untyped a, ?untyped b) -> String"), "{rbs}");
    assert!(!rbs.contains("def size: (untyped a) ->"), "{rbs}");
}

#[test]
fn a_later_def_is_the_one_that_runs() {
    app(WIDGET)
        .run_ruby(
            r##"
w = Widget.create!(name: "w")
raise "flavor: #{Widget.flavor}" unless Widget.flavor == "class-second"
raise "summary: #{w.summary}" unless w.summary == "second"
raise "size: #{w.size(1, 2)}" unless w.size(1, 2) == "two-arg" && w.size(1) == "two-arg"
puts "ALL OK"
"##,
        )
        .assert_passes();
}

/// A `def` before a class-body macro that generates the same name keeps
/// the behaviour it had: only a later `def` of the body replaces an
/// earlier one.
#[test]
fn a_def_before_a_macro_of_the_same_name_is_unchanged() {
    let widget = "class Widget < ApplicationRecord\n  def tag\n    \"own\"\n  end\n\n  attr_accessor :tag\nend\n";
    for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
        let model = emitted_model(widget, target);
        assert!(model.contains("\"own\""), "{target:?}:\n{model}");
    }
}

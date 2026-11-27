//! A concern included next to the one that overrides `destroy` must not hide
//! the override. Every model concern is seeded with the AR instance catalog
//! (so a bare `save` in its body types); the concern fold copies a module's
//! methods onto the model, and the seed must not count as a definition there.
//! Ruby resolves `destroy` to the concern that defines it whatever the order.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use emit_and_run::Overlay;
use roundhouse::project::BuildTarget;

const SCHEMA: &str = "ActiveRecord::Schema.define do\n  create_table :widgets do |t|\n    t.string :name\n  end\nend\n";

const RETIRABLE: &str = "module Retirable
  extend ActiveSupport::Concern

  def destroy
    return false if name == \"keep\"

    super()
    true
  end
end
";

const LABELLED: &str = "module Labelled
  extend ActiveSupport::Concern

  def label
    name.to_s
  end
end
";

const SERVICE: &str = "class Retirer
  def self.run(id)
    done = Widget.find(id).destroy
    verdict(done: done)
  end

  def self.verdict(done:)
    done ? :gone : :kept
  end
end
";

fn example(includes: &[&str]) -> Overlay {
    let body: String = includes.iter().map(|m| format!("  include {m}\n")).collect();
    emit_and_run::empty_app()
        .write("db/schema.rb", SCHEMA)
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n")
        .write("app/models/concerns/retirable.rb", RETIRABLE)
        .write("app/models/concerns/labelled.rb", LABELLED)
        .write("app/models/widget.rb", &format!("class Widget < ApplicationRecord\n{body}end\n"))
        .write("app/models/retirer.rb", SERVICE)
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::API\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
}

fn verdict_line(includes: &[&str]) -> String {
    let (emitted, errors) = example(includes).emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    let rbs = std::fs::read_to_string(emitted.join("app/models/retirer.rbs")).unwrap();
    rbs.lines()
        .find(|l| l.contains("def self.verdict:"))
        .unwrap_or_else(|| panic!("no verdict in:\n{rbs}"))
        .trim()
        .to_string()
}

#[test]
fn concern_included_after_the_override_keeps_its_type() {
    let line = verdict_line(&["Retirable", "Labelled"]);
    assert!(line.contains("done: bool"), "{line}");
}

#[test]
fn concern_included_before_the_override_keeps_its_type() {
    let line = verdict_line(&["Labelled", "Retirable"]);
    assert!(line.contains("done: bool"), "{line}");
}

#[test]
fn single_include_keeps_its_type() {
    let line = verdict_line(&["Retirable"]);
    assert!(line.contains("done: bool"), "{line}");
}

#[test]
fn the_override_runs_whatever_the_include_order() {
    let script = r#"
Widget.create!(name: "keep")
Widget.create!(name: "drop")
raise "kept" unless Retirer.run(1) == :kept && Widget.count == 2
raise "gone" unless Retirer.run(2) == :gone && Widget.count == 1
puts "override passed"
"#;
    example(&["Retirable", "Labelled"]).run_ruby(script).assert_passes();
}

#[test]
fn typescript_receives_the_boolean_too() {
    let (emitted, errors) = example(&["Retirable", "Labelled"]).emit(BuildTarget::Typescript);
    assert!(errors.is_empty(), "{errors:?}");
    let mut found = None;
    let mut stack = vec![emitted.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|n| n == "retirer.ts") {
                found = Some(std::fs::read_to_string(&path).unwrap());
            }
        }
    }
    let source = found.expect("retirer.ts emitted");
    let line = source.lines().find(|l| l.contains("static verdict")).expect("verdict");
    assert!(line.contains("done: boolean"), "{line}");
}

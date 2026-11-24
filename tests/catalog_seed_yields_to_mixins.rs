//! The catalog seeds every model with Rails' own `destroy` (and `save`,
//! `update`, ...). An override in an included concern or a parent class sits
//! ahead of the framework in Ruby's ancestor order, so it must type the call.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

const SCHEMA: &str = "ActiveRecord::Schema.define do\n  create_table :widgets do |t|\n    t.string :name\n  end\n  create_table :gadgets do |t|\n    t.string :name\n  end\nend\n";

const RETIRABLE: &str = "module Retirable
  extend ActiveSupport::Concern

  def destroy
    return false if name == \"keep\"

    super()
    true
  end
end
";

const SERVICE: &str = "class Retirer
  def self.widget(id)
    done = Widget.find(id).destroy
    verdict(done: done)
  end

  def self.gadget(id)
    done = Gadget.find(id).destroy
    other(done: done)
  end

  def self.verdict(done:)
    done == false ? :kept : :gone
  end

  def self.other(done:)
    done ? :gone : :kept
  end
end
";

fn example(application_record: &str, widget: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", SCHEMA)
        .write("app/models/application_record.rb", application_record)
        .write("app/models/concerns/retirable.rb", RETIRABLE)
        .write("app/models/widget.rb", widget)
        .write("app/models/gadget.rb", "class Gadget < ApplicationRecord\nend\n")
        .write("app/models/retirer.rb", SERVICE)
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::API\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
}

fn sidecar_line(overlay: emit_and_run::Overlay, needle: &str) -> String {
    let (emitted, errors) = overlay.emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    let rbs = std::fs::read_to_string(emitted.join("app/models/retirer.rbs")).unwrap();
    rbs.lines().find(|l| l.contains(needle)).unwrap_or_else(|| panic!("no `{needle}` in:\n{rbs}")).trim().to_string()
}

const PLAIN_AR: &str = "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n";

#[test]
fn included_concern_destroy_override_types_the_call() {
    let line = sidecar_line(
        example(PLAIN_AR, "class Widget < ApplicationRecord\n  include Retirable\nend\n"),
        "def self.verdict:",
    );
    assert!(line.contains("done: bool"), "{line}");
}

#[test]
fn parent_class_destroy_override_types_the_call() {
    let parent = "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\n  def destroy\n    super()\n    true\n  end\nend\n";
    let line = sidecar_line(
        example(parent, "class Widget < ApplicationRecord\nend\n"),
        "def self.verdict:",
    );
    assert!(line.contains("done: bool"), "{line}");
}

#[test]
fn model_without_an_override_keeps_the_catalog_type() {
    let line = sidecar_line(
        example(PLAIN_AR, "class Widget < ApplicationRecord\n  include Retirable\nend\n"),
        "def self.other:",
    );
    assert!(line.contains("done: ActiveRecord::Base"), "{line}");
}

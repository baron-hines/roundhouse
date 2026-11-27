#![allow(dead_code)]

pub const BASE: &str = "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n";

pub const MODEL: &str = r#"class Assignment < ApplicationRecord
  Transition = Data.define(:assignment, :changed, :notice)

  def transition(changed)
    Transition.new(assignment: self, changed: changed, notice: changed ? "saved" : nil)
  end

  def self.unchanged(assignment)
    Transition.new(assignment: assignment, changed: false, notice: nil)
  end
end
"#;

pub const SCHEMA: &str = r#"ActiveRecord::Schema.define(version: 1) do
  create_table :assignments do |t|
    t.string :name
  end
end
"#;

pub fn overlay() -> super::emit_and_run::Overlay {
    super::emit_and_run::empty_app()
        .write("app/models/application_record.rb", BASE)
        .write("db/schema.rb", SCHEMA)
        .write("app/models/assignment.rb", MODEL)
}

pub const ASSERTIONS: &str = r#"
assignment = Assignment.new(name: "one")
changed = assignment.transition(true)
raise "wrong nested class" unless changed.class == Assignment::Transition
raise "assignment was copied or coerced" unless changed.assignment.equal?(assignment)
raise "changed value lost" unless changed.changed == true && changed.notice == "saved"
unchanged = Assignment.unchanged(assignment)
raise "class-side factory lost false/nil" unless unchanged.changed == false && unchanged.notice.nil?
copy = changed.with(changed: false, notice: nil)
raise "with changed its receiver" unless changed.changed == true && changed.notice == "saved"
raise "with lost fields" unless copy.assignment.equal?(assignment) && copy.changed == false && copy.notice.nil?
raise "record is mutable" unless changed.frozen? && !changed.respond_to?(:notice=)
puts "model Data factory contract passed"
"#;

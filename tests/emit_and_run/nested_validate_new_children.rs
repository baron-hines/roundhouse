//! A nested-attributes owner validates a child built on its unloaded
//! has_many, and once: when the child's `belongs_to ..., autosave: true`
//! validates the changed owner back, the owner's errors are not nested a
//! level per pass, as Rails' autosave validation guards the cycle.
//! Ignored as a fork spec until the compiler does; the expected values
//! are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/kit.rb", r##"class Kit < ApplicationRecord
  has_many :parts
  accepts_nested_attributes_for :parts
  validates :name, presence: true
end
"##)
        .write("app/models/part.rb", r##"class Part < ApplicationRecord
  belongs_to :kit, autosave: true
  validates :code, presence: true
  validates :code, length: { minimum: 3 }
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    kit = Kit.create!(name: "radio")
    kit = Kit.find(kit.id)
    part = kit.parts.build(code: "")
    check("the child's save fails", part.save == false)
    check("the owner then fails too", kit.validate == false)
    check("with the child's errors", kit.errors.full_messages == ["Parts code can't be blank", "Parts code is too short (minimum is 3 characters)"])
    kit.name = ""
    check("a changed owner fails", kit.validate == false)
    check("its errors once", kit.errors.full_messages == ["Name can't be blank", "Parts kit name can't be blank", "Parts code can't be blank", "Parts code is too short (minimum is 3 characters)"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "kits", force: :cascade do |t|
    t.string "name"
  end
  create_table "parts", force: :cascade do |t|
    t.integer "kit_id"
    t.string "code"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: nested attributes"]
fn nested_owner_validates_new_children_once() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: nested attributes"]
fn nested_owner_validates_new_children_once_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

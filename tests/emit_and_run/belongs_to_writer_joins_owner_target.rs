//! A belongs_to writer hands the record to its new owner's has_many
//! target (the inverse it names), saved or not, so the owner counts and
//! holds it before anything is saved, as Rails does with
//! `has_many_inversing`. Ignored as a fork spec until the compiler does;
//! the expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/hive.rb", r##"class Hive < ApplicationRecord
  has_many :frames
end
"##)
        .write("app/models/frame.rb", r##"class Frame < ApplicationRecord
  belongs_to :hive, inverse_of: :frames
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    h = Hive.create!(name: "top")
    Frame.create!(slot: 1, hive: h)
    h = Hive.find(h.id)
    f = Frame.new(slot: 2)
    f.hive = h
    # Rails: the unsaved frame is in the owner's target, counted on top of the row
    check("held by the owner", h.frames.include?(f))
    check("size", h.frames.size == 2)

    n = Hive.new(name: "fresh")
    g = Frame.new(slot: 3)
    g.hive = n
    # Rails: a new owner holds it too, and saves it after itself
    check("new owner holds it", n.frames.include?(g))
    n.save!
    check("saved through the owner", g.persisted? && g.hive_id == n.id)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "hives", force: :cascade do |t|
    t.string "name"
  end
  create_table "frames", force: :cascade do |t|
    t.integer "slot"
    t.integer "hive_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: association targets"]
fn a_belongs_to_writer_puts_the_record_in_its_owners_target() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: association targets"]
fn a_belongs_to_writer_puts_the_record_in_its_owners_target_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

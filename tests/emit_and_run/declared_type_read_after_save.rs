//! After a save, an attribute declared with a type of the app's own
//! (`attribute :plan, RoutePlan::Type.new`) reads the value decoded from
//! what was written, not the object it was given: a field the encoding
//! drops is gone, and nothing is pending, as Rails' `changes_applied`
//! forgets the assignment. On `main` it fails first because the declared
//! type is ignored (the column reads as text). Ignored as a fork spec
//! until the compiler does; the expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/route_plan.rb", r##"class RoutePlan
  attr_reader :stops, :note

  def initialize(stops, note = nil)
    @stops = stops
    @note = note
  end

  def ==(other)
    other.is_a?(RoutePlan) && stops == other.stops && note == other.note
  end

  class Type < ActiveRecord::Type::Value
    def cast(value)
      value
    end

    def deserialize(raw)
      raw.nil? ? nil : RoutePlan.new(raw.split(","))
    end

    def serialize(value)
      value.nil? ? nil : value.stops.join(",")
    end
  end
end
"##)
        .write("app/models/trip.rb", r##"class Trip < ApplicationRecord
  attribute :plan, RoutePlan::Type.new
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    given = RoutePlan.new(["a", "b"], "detour")
    t = Trip.new(name: "loop", plan: given)
    check("before the save it holds the given object", t.plan.equal?(given))
    t.save!
    check("after it, the decoded one", !t.plan.equal?(given) && t.plan.stops == ["a", "b"] && t.plan.note.nil?)
    check("nothing pending", !t.changed?)
    t.update!(plan: RoutePlan.new(["c"], "again"))
    check("update! too", t.plan.note.nil? && t.plan.stops == ["c"])
    t.update!(name: "line")
    check("a later save writes only name", t.saved_changes.keys == ["name"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "trips", force: :cascade do |t|
    t.string "name"
    t.text "plan"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: attribute types"]
fn declared_type_reads_the_decoded_value_after_save() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: attribute types"]
fn declared_type_reads_the_decoded_value_after_save_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

//! `errors.group_by_attribute`: each attribute with errors, in the order
//! of its first error, to its `ActiveModel::Error`s. The probe's expected
//! values are ActiveRecord 8.1.4's answers for the same model and records.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/hike.rb", r##"class Hike < ApplicationRecord
  validates :trail, presence: true
  validates :km, numericality: { greater_than: 0 }
  validate :km_is_sensible

  def km_is_sensible
    errors.add(:km, "is unrealistic") if km && km > 500
    errors.add(:trail, :too_long) if trail && trail.length > 8
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  # Rails: [[:trail, :km], ["Trail can't be blank"], ["must be greater than 0"]]
  def self.grouped_by_first_error
    h = Hike.new(trail: "", km: -1)
    h.valid?
    groups = h.errors.group_by_attribute
    [groups.keys, groups[:trail].map(&:full_message), groups[:km].map(&:message)]
  end

  # Rails: [[:km, :trail], [2, 1], [:greater_than, "is unrealistic"]]
  def self.several_errors_on_one_attribute
    h = Hike.new(trail: "ridgeline-loop", km: 900)
    h.valid?
    h.errors.add(:km, :greater_than, count: 0)
    groups = h.errors.group_by_attribute
    [groups.keys, groups.values.map(&:size), groups[:km].map(&:type).reverse]
  end

  # Rails: [{}, true]
  def self.valid_record_has_no_groups
    h = Hike.new(trail: "coast", km: 12)
    ok = h.valid?
    [h.errors.group_by_attribute, ok]
  end

  def self.run
    check("grouped_by_first_error", grouped_by_first_error == [[:trail, :km], ["Trail can't be blank"], ["must be greater than 0"]])
    check("several_errors_on_one_attribute", several_errors_on_one_attribute == [[:km, :trail], [2, 1], [:greater_than, "is unrealistic"]])
    check("valid_record_has_no_groups", valid_record_has_no_groups == [{}, true])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "hikes", force: :cascade do |t|
    t.string "trail"
    t.integer "km"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn errors_group_by_attribute_answers_as_rails_does() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn errors_group_by_attribute_answers_as_rails_does_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

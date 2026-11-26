//! A decimal column's `read_attribute_before_type_cast` and
//! `<column>_before_type_cast` answer the value the app assigned, not the
//! cast one, so a validation reading it refuses `"heavy"`, as ActiveModel's
//! attribute keeps the value before its cast. Ignored as a fork spec until
//! the compiler does; the expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/parcel.rb", r##"class Parcel < ApplicationRecord
  validate :weight_is_a_number

  def weight_is_a_number
    raw = read_attribute_before_type_cast(:weight)
    errors.add(:weight, "is not a number") unless raw.nil? || raw.to_s.match?(/\A\d+(\.\d+)?\z/)
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    p = Parcel.new(weight: "heavy")
    check("the cast value", p.weight == BigDecimal("0"))
    check("read_attribute_before_type_cast answers the assignment", p.read_attribute_before_type_cast(:weight) == "heavy")
    check("the column's before_type_cast too", p.weight_before_type_cast == "heavy")
    check("the check refuses it", !p.valid? && p.errors[:weight] == ["is not a number"])
    p.weight = "2.5"
    check("a numeric String is kept as given", p.weight_before_type_cast == "2.5" && p.weight == BigDecimal("2.5"))
    check("and accepted", p.valid?)
    p.weight = 1.25
    check("a Float is kept as given", p.weight_before_type_cast == 1.25)
    p.weight = nil
    check("nil is nil", p.weight_before_type_cast.nil?)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "parcels", force: :cascade do |t|
    t.decimal "weight", precision: 8, scale: 3
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: attribute types"]
fn decimal_before_type_cast_answers_the_assignment() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: attribute types"]
fn decimal_before_type_cast_answers_the_assignment_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

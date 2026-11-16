//! A concern's `included` block that declares `cattr_accessor` /
//! `mattr_reader` with defaults: every includer gets the class and
//! instance readers, and a class-side write is seen by its instances.
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/concerns/capped.rb", r##"module Capped
  extend ActiveSupport::Concern

  included do
    cattr_accessor :cap, default: 3
    mattr_reader :unit, default: "cm"
  end

  def over_cap?
    length > cap
  end

  def with_unit
    "#{length}#{unit}"
  end
end
"##)
        .write("app/models/rope.rb", r##"class Rope < ApplicationRecord
  include Capped
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  # Rails: [3, "cm", true, false, "4cm"]
  def self.class_attributes_from_the_block
    rope = Rope.create!(length: 4)
    before = [Rope.cap, Rope.unit, rope.over_cap?]
    Rope.cap = 5
    before + [rope.over_cap?, rope.with_unit]
  end

  def self.run
    Rope.delete_all
    check("class_attributes_from_the_block", class_attributes_from_the_block == [3, "cm", true, false, "4cm"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "ropes", force: :cascade do |t|
    t.integer "length"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_concerns_included_cattr_accessor_reaches_its_includer() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_concerns_included_cattr_accessor_reaches_its_includer_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

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
        .write("app/models/concerns/seeded.rb", r##"module Seeded
  extend ActiveSupport::Concern

  included do
    cattr_accessor :quantity, default: 3
    cattr_reader :unit_size, default: 3
    cattr_writer :unit_size
  end
end
"##)
        .write("app/models/crate.rb", r##"class Crate < ApplicationRecord
  cattr_accessor :quantity, default: 1
  include Seeded
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

  # Rails: 3 — the class body runs top to bottom, so the concern's seed
  # (executed at the `include`) replaces the model's earlier one.
  def self.concern_seed_after_the_models_own
    Crate.quantity
  end

  # Rails: 3 — a `cattr_writer` with no default does not replace the
  # seed `cattr_reader` set before it.
  def self.seed_without_a_default_keeps_the_earlier
    Crate.unit_size
  end

  def self.run
    Rope.delete_all
    check("class_attributes_from_the_block", class_attributes_from_the_block == [3, "cm", true, false, "4cm"])
    check("concern_seed_after_the_models_own", concern_seed_after_the_models_own == 3)
    check("seed_without_a_default_keeps_the_earlier", seed_without_a_default_keeps_the_earlier == 3)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "ropes", force: :cascade do |t|
    t.integer "length"
  end
  create_table "crates", force: :cascade do |t|
    t.string "name"
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

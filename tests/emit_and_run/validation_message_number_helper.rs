//! A `message: proc { ... }` runs with the class body's `self`, so a model
//! that `extend`s ActiveSupport::NumberHelper formats its limit with
//! `number_to_delimited` inside the message, as Rails builds it. Ignored
//! as a fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/crate_order.rb", r##"class CrateOrder < ApplicationRecord
  extend ActiveSupport::NumberHelper

  MAX_UNITS = 25_000

  validates :units, numericality: {
    less_than_or_equal_to: MAX_UNITS,
    message: proc { "must be #{number_to_delimited(MAX_UNITS)} or fewer" }
  }
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    check("a small order is valid", CrateOrder.new(units: 40).valid?)
    big = CrateOrder.new(units: 30_000)
    check("a big one is not", !big.valid?)
    check("its message is formatted", big.errors.full_messages == ["Units must be 25,000 or fewer"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "crate_orders", force: :cascade do |t|
    t.integer "units"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: validations"]
fn validation_message_proc_reads_the_class_extend() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: validations"]
fn validation_message_proc_reads_the_class_extend_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

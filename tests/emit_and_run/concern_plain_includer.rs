//! A concern included into a plain Active Model class: its `included`
//! block's `attr_accessor`, `validates` and `validate :method` register on
//! the includer, as Rails runs the block there.
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/concerns/dated.rb", r##"module Dated
  extend ActiveSupport::Concern

  included do
    attr_accessor :day
    validates :day, presence: true
    validate :day_in_month
  end

  private

  def day_in_month
    errors.add(:day, "is past the month") if day.to_i > 31
  end
end
"##)
        .write("app/models/booking.rb", r##"class Booking
  include ActiveModel::Model
  include Dated
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  # Rails: [true, false, ["Day can't be blank"], false, ["Day is past the month"]]
  def self.plain_includer
    good = Booking.new(day: 12)
    blank = Booking.new
    late = Booking.new(day: 40)
    [good.valid?, blank.valid?, blank.errors.full_messages, late.valid?, late.errors.full_messages]
  end

  def self.run
    check("plain_includer", plain_includer == [true, false, ["Day can't be blank"], false, ["Day is past the month"]])
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_concerns_validations_register_on_a_plain_active_model_class() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_concerns_validations_register_on_a_plain_active_model_class_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

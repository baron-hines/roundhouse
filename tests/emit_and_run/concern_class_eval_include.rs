//! A plain module whose `self.included(base)` runs `base.class_eval {
//! include Concern }`: the concern lands on the includer as if the model
//! had included it, with its `included` block and its `class_methods`.
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/concerns/weighed.rb", r##"module Weighed
  extend ActiveSupport::Concern

  included do
    scope :heavy, -> { where("grams > ?", 500) }
  end

  class_methods do
    def heavy_total
      heavy.sum(:grams)
    end
  end

  def kilos
    grams / 1000.0
  end
end
"##)
        .write("app/models/concerns/shippable.rb", r##"module Shippable
  def self.included(base)
    base.class_eval do
      include Weighed
    end
  end
end
"##)
        .write("app/models/parcel.rb", r##"class Parcel < ApplicationRecord
  include Shippable
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  # Rails: [2400, 1.5]
  def self.class_eval_include
    Parcel.create!(grams: 400)
    Parcel.create!(grams: 1500)
    Parcel.create!(grams: 900)
    [Parcel.heavy_total, Parcel.order(:grams).last.kilos]
  end

  def self.run
    Parcel.delete_all
    check("class_eval_include", class_eval_include == [2400, 1.5])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "parcels", force: :cascade do |t|
    t.integer "grams"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_concern_included_through_class_eval_reaches_the_model() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_concern_included_through_class_eval_reaches_the_model_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

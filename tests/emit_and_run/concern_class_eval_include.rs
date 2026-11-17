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
        .write("app/models/concerns/stackable.rb", r##"module Stackable
  def stack_height
    2
  end
end
"##)
        .write("app/models/carton.rb", r##"class Carton < ApplicationRecord
  include Shippable, Stackable
end
"##)
        .write("app/models/concerns/wrapped.rb", r##"module Wrapped
  extend ActiveSupport::Concern

  included do
    cattr_accessor :layers, default: 1
  end
end
"##)
        .write("app/models/concerns/boxed.rb", r##"module Boxed
  extend ActiveSupport::Concern

  included do
    cattr_accessor :layers, default: 2
  end
end
"##)
        .write("app/models/concerns/packable.rb", r##"module Packable
  def self.included(base)
    base.include Wrapped, Boxed
  end
end
"##)
        .write("app/models/pallet.rb", r##"class Pallet < ApplicationRecord
  include Packable
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

  # Rails: [700, 2] — the hook runs for a module named among several in
  # one `include`.
  def self.hook_in_a_multi_argument_include
    Carton.create!(grams: 300)
    Carton.create!(grams: 700)
    [Carton.heavy_total, Carton.first.stack_height]
  end

  # Rails: 1 — the hook's `base.include Wrapped, Boxed` runs Boxed's
  # block first, so Wrapped's seed is the one left.
  def self.hook_include_with_two_arguments
    Pallet.layers
  end

  def self.run
    Parcel.delete_all
    Carton.delete_all
    check("class_eval_include", class_eval_include == [2400, 1.5])
    check("hook_in_a_multi_argument_include", hook_in_a_multi_argument_include == [700, 2])
    check("hook_include_with_two_arguments", hook_include_with_two_arguments == 1)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "parcels", force: :cascade do |t|
    t.integer "grams"
  end
  create_table "cartons", force: :cascade do |t|
    t.integer "grams"
  end
  create_table "pallets", force: :cascade do |t|
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

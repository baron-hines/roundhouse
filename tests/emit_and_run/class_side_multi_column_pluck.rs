//! A bare `pluck(:a, :b)` or `pick(:a, :b)` in a class method answers
//! rows of both columns, as Rails' `Querying` hands every argument to
//! `all`. The class side took one column, so the call raised
//! ArgumentError where `check` was clean. The expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/gauge.rb", r##"class Gauge < ApplicationRecord
  def self.levels
    pluck(:name, :level).to_h
  end

  def self.first_reading
    order(:id).pick(:name, :level)
  end

  def self.lowest
    pick(:level)
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    Gauge.create!(name: "oil", level: 3)
    Gauge.create!(name: "fuel", level: 7)
    # Rails: {"oil" => 3, "fuel" => 7}, ["oil", 3], 3
    check("pluck of two columns", Gauge.levels == { "oil" => 3, "fuel" => 7 })
    check("pick of two columns", Gauge.first_reading == ["oil", 3])
    check("pick of one column", Gauge.lowest == 3)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "gauges", force: :cascade do |t|
    t.string "name"
    t.integer "level"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn class_side_pluck_of_several_columns_answers_rows() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn class_side_pluck_of_several_columns_answers_rows_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

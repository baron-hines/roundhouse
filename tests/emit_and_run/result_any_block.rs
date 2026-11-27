//! `ActiveRecord::Result` answers Enumerable's `any?` over its rows as
//! Hashes, with and without a block, as Rails' Result does. Ignored as a
//! fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/beacon.rb", r##"class Beacon < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    conn = Beacon.connection
    empty = conn.select_all("SELECT state FROM beacons")
    check("empty, no block", empty.any? == false)
    check("empty, a block", empty.any? { |row| true } == false)
    Beacon.create!(state: "on")
    full = conn.select_all("SELECT state FROM beacons")
    # Rails: true, true, false
    check("filled, no block", full.any? == true)
    check("filled, a matching block", full.any? { |row| row["state"] == "on" } == true)
    check("filled, a missing block", full.any? { |row| row["state"] == "off" } == false)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "beacons", force: :cascade do |t|
    t.string "state"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: relation queries"]
fn a_result_answers_any_over_its_rows() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: relation queries"]
fn a_result_answers_any_over_its_rows_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

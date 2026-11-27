//! `record[key]` reads the attribute a key names, by Symbol or String,
//! with its cast type, answers `nil` for a key that names no attribute,
//! and a model's own `[]` override answers instead, as Rails' `[]` and
//! Ruby's dispatch do. Ignored as a fork spec until the compiler does; the
//! expected values are ActiveRecord 8.1.4's answers.

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
        .write("app/models/relay.rb", r##"class Relay < ApplicationRecord
  def [](key)
    key.to_s == "range" ? range * 10 : super
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.read(record, key)
    record[key]
  end

  def self.run
    b = Beacon.create!(code: "b1", range: 4)
    # Rails: "b1", 4, 4, nil
    check("Symbol key", read(b, :code) == "b1")
    check("cast type", read(b, :range) == 4 && read(b, "range") == 4)
    check("no such attribute", read(b, :missing).nil?)
    r = Relay.create!(code: "r1", range: 3)
    # Rails: 30, "r1"
    check("own [] override", read(r, :range) == 30 && read(r, :code) == "r1")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "beacons", force: :cascade do |t|
    t.string "code"
    t.integer "range"
  end
  create_table "relays", force: :cascade do |t|
    t.string "code"
    t.integer "range"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: record queries"]
fn record_brackets_read_attributes_and_honor_an_override() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: record queries"]
fn record_brackets_read_attributes_and_honor_an_override_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

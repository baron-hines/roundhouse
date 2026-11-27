//! A json column changed in place is a change to save: a read alone is no
//! change (the original is the value the row gave it, even when its JSON
//! text is not what `serialize` writes; that half passes on `main`), a
//! mutation of the Hash it answered makes the record `changed?`, and
//! `save!` writes it, as Rails asks the type's `changed_in_place?` with
//! the row's value.
//! Ignored as a fork spec until the compiler does; the expected values
//! are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/station.rb", r##"class Station < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    s = Station.create!(name: "desk", paper: { "size" => "a4", "tray" => 1 })
    Station.where(id: s.id).update_all("paper = '{\"tray\": 1, \"size\": \"a4\"}'")
    s = Station.find(s.id)
    check("a read is no change", s.paper["size"] == "a4" && !s.changed?)
    s.paper["size"] = "letter"
    check("a mutation in place is a change", s.changed?)
    s.save!
    check("save writes it", Station.find(s.id).paper == { "size" => "letter", "tray" => 1 })
    check("nothing is pending after the save", !s.changed?)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "stations", force: :cascade do |t|
    t.string "name"
    t.json "paper"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: attribute types"]
fn json_changed_in_place_is_saved() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: attribute types"]
fn json_changed_in_place_is_saved_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

//! `save(touch: false)` and `save!(touch: false)` write an update
//! without stamping `updated_at`, while a plain save stamps it, as Rails'
//! Timestamp#create_or_update does. Ignored as a fork spec until the
//! compiler does; the expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/note.rb", r##"class Note < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    past = Time.utc(2020, 1, 2, 3, 4, 5)
    n = Note.create!(title: "a")
    n.update_columns(updated_at: past)
    n.title = "b"
    check("save answers true", n.save(touch: false) == true)
    n = Note.find(n.id)
    check("the update is written", n.title == "b")
    check("save(touch: false) keeps updated_at", n.updated_at == past)
    n.title = "c"
    n.save!(touch: false)
    saved = Note.find(n.id)
    check("save!(touch: false) writes the update", saved.title == "c")
    check("save!(touch: false) keeps updated_at", saved.updated_at == past)
    n.title = "d"
    n.save
    check("a plain save stamps", Note.find(n.id).updated_at > past)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "notes", force: :cascade do |t|
    t.string "title"
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: persistence"]
fn save_touch_false_keeps_updated_at() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: persistence"]
fn save_touch_false_keeps_updated_at_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

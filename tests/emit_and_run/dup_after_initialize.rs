//! `dup` runs `after_initialize` with the key already reset but while
//! `new_record?` and the timestamps still answer the source's, then marks
//! the copy new and clears its timestamps, as Rails' `initialize_dup`
//! does. Ignored as a fork spec until the compiler does; the expected
//! values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/entry.rb", r##"class Entry < ApplicationRecord
  SEEN = []
  after_initialize :note_state

  def note_state
    SEEN << [new_record?, id.nil?, created_at.nil?]
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    saved = Entry.create!(title: "a")
    Entry::SEEN.clear
    copy = saved.dup
    # Rails: [[false, true, false]]
    check("the callback sees the source's state", Entry::SEEN == [[false, true, false]])
    check("the copy is new", copy.new_record? == true)
    check("the copy has no key", copy.id.nil?)
    check("the copy has no timestamps", copy.created_at.nil?)
    fresh = Entry.new(title: "b")
    Entry::SEEN.clear
    fresh.dup
    # Rails: [[true, true, true]]
    check("a new source's copy", Entry::SEEN == [[true, true, true]])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "entries", force: :cascade do |t|
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
fn dup_runs_after_initialize_on_the_source_state() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: persistence"]
fn dup_runs_after_initialize_on_the_source_state_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

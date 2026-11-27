//! `Model.delete(id_or_array)` deletes the rows of those keys with one
//! DELETE, running no callbacks, and answers the number of rows deleted,
//! as Rails' `Relation#delete` does (Querying delegates the class's
//! `delete` to `all`).
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/draft.rb", r##"class Draft < ApplicationRecord
  before_destroy :remember

  def self.destroyed
    @destroyed ||= []
  end

  def remember
    Draft.destroyed << title
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, actual, expected)
    raise "#{label}: #{actual.inspect}, Rails answers #{expected.inspect}" unless actual == expected
  end

  def self.run
    first = Draft.create!(title: "intro")
    second = Draft.create!(title: "outline")
    third = Draft.create!(title: "notes")
    kept = Draft.create!(title: "chapter")
    Draft.create!(title: "epilogue")
    check("an id", Draft.delete(first.id), 1)
    check("an Array with a missing key", Draft.delete([second.id, third.id, 0]), 2)
    check("a missing id", Draft.delete(0), 0)
    check("an Array of none", Draft.delete([]), 0)
    check("rows left", Draft.order(:id).pluck(:title), ["chapter", "epilogue"])
    check("no callbacks", Draft.destroyed, [])
    check("the record", [first.persisted?, kept.reload.title], [true, "chapter"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "drafts", force: :cascade do |t|
    t.string "title"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn model_delete_removes_rows_by_key_without_callbacks() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn model_delete_removes_rows_by_key_without_callbacks_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

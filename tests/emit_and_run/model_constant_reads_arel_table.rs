//! A model constant built from `arel_table` (`COLUMNS = [:id, :title].map
//! { |c| arel_table[c] }`), and a constant built from that one, load with
//! the model and select through it, as Rails answers `arel_table` from the
//! class body's first line. Ignored as a fork spec until the compiler
//! does; the expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/memo.rb", r##"class Memo < ApplicationRecord
  SUMMARY_COLUMNS = [:id, :title].map { |name| arel_table[name] }
  LISTED_COLUMNS = SUMMARY_COLUMNS + [arel_table[:pinned]]

  def self.summaries
    select(SUMMARY_COLUMNS).order(:id)
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    Memo.create!(title: "first", body: "long", pinned: true)
    check("the constant holds Arel attributes", Memo::SUMMARY_COLUMNS.map(&:name) == ["id", "title"])
    check("the one built from it too", Memo::LISTED_COLUMNS.map(&:name) == ["id", "title", "pinned"])
    row = Memo.summaries.first
    check("the select reads them", row.title == "first" && !row.has_attribute?(:body))
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "memos", force: :cascade do |t|
    t.string "title"
    t.string "body"
    t.boolean "pinned"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: ruby semantics"]
fn model_constant_reads_arel_table_at_load() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn model_constant_reads_arel_table_at_load_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

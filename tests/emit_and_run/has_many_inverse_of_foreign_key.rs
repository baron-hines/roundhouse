//! A `has_many` or `has_one` with `inverse_of:` and no `foreign_key:`
//! takes its key from the inverse `belongs_to`, as Rails'
//! `derive_foreign_key` does: `has_many :citations, inverse_of:
//! :source_note` reads `citations.source_note_id`, and an inverse with
//! its own `foreign_key:` lends that column. The key came from the
//! owner's name (`note_id`), a column the table does not have. The
//! expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/note.rb", r##"class Note < ApplicationRecord
  has_many :citations, inverse_of: :source_note, dependent: :destroy
  has_many :echoes, class_name: "Citation", inverse_of: :echoed
  has_one :summary, inverse_of: :subject_note
end
"##)
        .write("app/models/citation.rb", r##"class Citation < ApplicationRecord
  belongs_to :source_note, class_name: "Note", optional: true
  belongs_to :echoed, class_name: "Note", foreign_key: :echo_ref, optional: true
end
"##)
        .write("app/models/summary.rb", r##"class Summary < ApplicationRecord
  belongs_to :subject_note, class_name: "Note"
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    a = Note.create!(title: "a")
    b = Note.create!(title: "b")
    Citation.create!(label: "one", source_note_id: a.id)
    Citation.create!(label: "two", source_note_id: a.id, echo_ref: b.id)
    a.citations.create!(label: "three")
    Summary.create!(text: "short", subject_note_id: b.id)
    # Rails: ["one", "three", "two"], 0, ["two"], "short", nil, 3
    check("has_many reader", a.citations.map(&:label).sort == ["one", "three", "two"])
    check("has_many count", b.citations.count == 0)
    check("key from the inverse's foreign_key:", b.echoes.map(&:label) == ["two"])
    check("has_one reader", b.summary.text == "short")
    check("has_one reader without a row", a.summary.nil?)
    check("created through the association", Citation.where(source_note_id: a.id).count == 3)
    a.destroy
    # Rails: 0
    check("dependent destroy", Citation.where(source_note_id: a.id).count == 0)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "notes", force: :cascade do |t|
    t.string "title"
  end
  create_table "citations", force: :cascade do |t|
    t.string "label"
    t.integer "source_note_id"
    t.integer "echo_ref"
  end
  create_table "summaries", force: :cascade do |t|
    t.string "text"
    t.integer "subject_note_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn has_many_inverse_of_takes_the_inverse_foreign_key() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn has_many_inverse_of_takes_the_inverse_foreign_key_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

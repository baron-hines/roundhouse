//! A required polymorphic `belongs_to` checks that its record exists, as
//! Rails' presence validation does: assigning the record itself is valid
//! even when it is unsaved (and `save` writes it first), while a
//! `_type`/`_id` pair naming no row is not. The expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/library.rb", r##"class Library < ApplicationRecord
end
"##)
        .write("app/models/museum.rb", r##"class Museum < ApplicationRecord
  validates :name, presence: true
end
"##)
        .write("app/models/ticket.rb", r##"class Ticket < ApplicationRecord
  self.primary_key = "code"
end
"##)
        .write("app/models/note.rb", r##"class Note < ApplicationRecord
  belongs_to :subject, polymorphic: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    # Rails: a key pair naming no row is "Subject must exist".
    missing = Note.new(subject_type: "Library", subject_id: 999)
    check("missing record invalid", !missing.valid?)
    check("missing record message", missing.errors[:subject] == ["must exist"])
    # Rails: a key pair naming a saved row is valid.
    saved = Library.create!(name: "main")
    found = Note.new(subject_type: "Library", subject_id: saved.id)
    check("saved record valid", found.valid?)
    # Rails: the record assigned through the writer is valid even when
    # it is new, and `save` writes it first.
    fresh = Library.new(name: "branch")
    assigned = Note.new(body: "a")
    assigned.subject = fresh
    check("assigned unsaved record valid", assigned.valid?)
    check("assigned unsaved record read back", assigned.subject.equal?(fresh))
    assigned.save!
    check("associated record saved", !fresh.new_record? && Library.find(fresh.id).name == "branch")
    stored = Note.find(assigned.id)
    check("note points at it", stored.subject_id == fresh.id && stored.subject_type == "Library")
    # Rails: writing the key again to a row that is gone drops the record
    # the writer kept.
    moved = Note.new
    moved.subject = Library.create!(name: "west")
    moved.subject_id = 999
    check("rewritten key invalid", !moved.valid?)
    # Rails: writing the type column does too.
    Museum.create!(name: "the met")
    retyped = Note.new
    retyped.subject = Library.create!(name: "east")
    retyped.subject_type = "Museum"
    check("rewritten type invalid", !retyped.valid?)
    # Rails: a key with no type names no record.
    check("key without a type invalid", !Note.new(subject_id: fresh.id).valid?)
    # Rails: a record keyed by its own column, assigned unsaved with its
    # key set, is the record the writer was given.
    ticket = Ticket.new(code: "T-7", title: "leak")
    keyed = Note.new(body: "k")
    keyed.subject = ticket
    check("assigned record with its own key valid", keyed.valid? && keyed.subject.equal?(ticket))
    # Rails: an assigned record that fails to save leaves the note saved
    # anyway; the presence check passed on the record before the save.
    failing = Note.new(body: "f")
    failing.subject = Museum.new
    check("failed record save, note still saved", failing.save == true && failing.persisted? && Museum.count == 1)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "libraries", force: :cascade do |t|
    t.string "name"
  end
  create_table "museums", force: :cascade do |t|
    t.string "name"
  end
  create_table "tickets", id: false, force: :cascade do |t|
    t.string "code", primary_key: true
    t.string "title"
  end
  create_table "notes", force: :cascade do |t|
    t.string "body"
    t.string "subject_type"
    t.integer "subject_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_required_polymorphic_belongs_to_checks_its_record_exists() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_required_polymorphic_belongs_to_checks_its_record_exists_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

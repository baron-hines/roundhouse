//! `restore_attributes` writes every pending change back, as Rails does.
//! The runtime lacked it while `check` stayed clean, so the emitted
//! program raised NoMethodError. The probe's expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/library.rb", r##"class Library < ApplicationRecord
  has_many :books
end
"##)
        .write("app/models/book.rb", r##"class Book < ApplicationRecord
end
"##)
        .write("app/models/note.rb", r##"class Note < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  # Rails: ["draft", "plain", false, ["body", "title"]]
  def self.restore_attributes
    note = Note.create!(title: "draft", body: "plain")
    note.title = "final"
    note.body = "rich"
    restored = note.restore_attributes
    [note.title, note.body, note.changed?, restored.sort]
  end

  # Rails: ["draft", false, []]
  def self.restore_attributes_with_nothing_pending
    note = Note.create!(title: "draft", body: "plain")
    restored = note.restore_attributes
    [note.title, note.changed?, restored]
  end

  def self.run
    check("restore_attributes", restore_attributes == ["draft", "plain", false, ["body", "title"]])
    check("restore_attributes_with_nothing_pending", restore_attributes_with_nothing_pending == ["draft", false, []])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "libraries", force: :cascade do |t|
    t.string "name"
  end
  create_table "books", force: :cascade do |t|
    t.string "title"
    t.integer "library_id"
  end
  create_table "notes", force: :cascade do |t|
    t.string "title"
    t.string "body"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn restore_attributes_answers_as_rails_does() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn restore_attributes_answers_as_rails_does_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

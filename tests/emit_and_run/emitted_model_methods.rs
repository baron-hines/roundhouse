//! A call on an app model names a method the emitted Ruby defines, or the
//! emit reports it. `restore_attributes` is one Rails answers that the
//! runtime lacked while `check` stayed clean, so the emitted program raised
//! NoMethodError; the probe's expected values are ActiveRecord 8.1.4's
//! answers. A call the runtime still lacks (`association(:name)` read for
//! its own sake) is an error now, where it used to raise when run.

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

  # Rails: [false, "plain"]
  def self.restore_attributes_of_a_partial_row
    Note.create!(title: "kept", body: "plain")
    note = Note.select(:id, :title).where(title: "kept").first
    note.body = nil
    note.restore_attributes
    after = note.changed?
    note.save!
    [after, Note.find(note.id).body]
  end

  def self.run
    check("restore_attributes_of_a_partial_row", restore_attributes_of_a_partial_row == [false, "plain"])
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

/// `association(:books)` read for its own sake has no runtime behind it:
/// the emit reports the call, on the Ruby family and on Spinel, instead of
/// writing a program that raises NoMethodError there.
#[test]
fn a_model_call_the_emitted_ruby_lacks_is_an_error() {
    for target in [roundhouse::project::BuildTarget::Ruby, roundhouse::project::BuildTarget::Spinel] {
        let (_emitted, errors) = app()
            .write("app/models/shelf_report.rb", "module ShelfReport\n  def self.association_of(id)\n    Library.find(id).association(:books)\n  end\nend\n")
            .emit(target);
        assert!(
            errors.iter().any(|e| e.contains("`Library#association` is called, but the emitted Ruby defines no such method")),
            "{target:?}: {errors:?}"
        );
    }
}

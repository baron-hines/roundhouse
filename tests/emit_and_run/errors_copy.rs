//! `errors.copy!(other)` replaces a record's errors with copies of the
//! other's: its own are dropped, each copy keeps its attribute, type and
//! message, and its full message names the attribute as the copying
//! model does. The probe's expected values are ActiveRecord 8.1.4's.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/sketch.rb", r##"class Sketch < ApplicationRecord
  validates :caption, presence: true
  validates :ink_colour, inclusion: { in: %w[black sepia] }
end
"##)
        .write("app/models/poster.rb", r##"class Poster < ApplicationRecord
  validates :caption, length: { maximum: 3 }
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  # Rails: [["Caption can't be blank", "Ink colour is not included in the list"], [:caption, :ink_colour]]
  def self.copy_replaces_own_errors
    s = Sketch.new(caption: "", ink_colour: "red")
    s.valid?
    c = Poster.new(caption: "too long")
    c.valid?
    c.errors.copy!(s.errors)
    [c.errors.full_messages, c.errors.attribute_names]
  end

  # Rails: [["can't be blank"], 2, 2]
  def self.copies_are_independent
    s = Sketch.new(caption: "", ink_colour: "red")
    s.valid?
    c = Poster.new(caption: "ok")
    c.errors.copy!(s.errors)
    c.errors.delete(:ink_colour)
    [c.errors[:caption], s.errors.size, s.errors.count]
  end

  # Rails: [[], true]
  def self.copy_of_no_errors_clears
    s = Sketch.new(caption: "fine", ink_colour: "black")
    s.valid?
    c = Poster.new(caption: "too long")
    c.valid?
    c.errors.copy!(s.errors)
    [c.errors.full_messages, c.errors.empty?]
  end

  def self.run
    check("copy_replaces_own_errors", copy_replaces_own_errors == [["Caption can't be blank", "Ink colour is not included in the list"], [:caption, :ink_colour]])
    check("copies_are_independent", copies_are_independent == [["can't be blank"], 2, 2])
    check("copy_of_no_errors_clears", copy_of_no_errors_clears == [[], true])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "sketches", force: :cascade do |t|
    t.string "caption"
    t.string "ink_colour"
  end
  create_table "posters", force: :cascade do |t|
    t.string "caption"
    t.string "ink_colour"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn errors_copy_answers_as_rails_does() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

/// Rails' `copy!` takes an `Errors`; given a whole record it stores that
/// record's `Errors` where its Array goes, so that shape stays refused.
#[test]
fn errors_copy_of_a_record_stays_refused() {
    let (_emitted, errors) = app()
        .write("app/models/copier.rb", "class Copier\n  def self.copy_record\n    sketch = Sketch.new(caption: \"\")\n    poster = Poster.new(caption: \"x\")\n    poster.errors.copy!(sketch)\n    poster\n  end\nend\n")
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("only another record's `errors` is grounded")), "{errors:#?}");
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn errors_copy_answers_as_rails_does_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

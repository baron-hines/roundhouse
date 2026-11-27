//! A required `belongs_to` given an unsaved record is valid before either
//! side is saved, as Rails validates the association's presence through
//! its reader: `Card.new(deck: Deck.new).valid?` is true. The check also
//! read the key, which an unsaved record leaves unset, so it reported
//! "Deck must exist". A key naming no row, or none at all, still fails.
//! The expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/deck.rb", r##"class Deck < ApplicationRecord
  has_many :cards
end
"##)
        .write("app/models/card.rb", r##"class Card < ApplicationRecord
  belongs_to :deck
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    built = Card.new(face: "ace", deck: Deck.new(title: "red"))
    assigned = Card.new(face: "two")
    assigned.deck = Deck.new(title: "blue")
    # Rails: true, true, 0
    check("unsaved record given to new", built.valid?)
    check("unsaved record given to the writer", assigned.valid?)
    check("valid? saves nothing", Deck.count == 0)
    built.save!
    # Rails: 1, "red"
    check("save saves the record first", Deck.count == 1)
    check("the key follows", Card.find(built.id).deck.title == "red")
    missing = Card.new(face: "three", deck_id: 999)
    # Rails: false, ["Deck must exist"], false
    check("a key naming no row", !missing.valid?)
    check("its message", missing.errors.full_messages == ["Deck must exist"])
    check("no record at all", !Card.new(face: "four").valid?)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "decks", force: :cascade do |t|
    t.string "title"
  end
  create_table "cards", force: :cascade do |t|
    t.string "face"
    t.integer "deck_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn required_belongs_to_with_an_unsaved_record_is_valid() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn required_belongs_to_with_an_unsaved_record_is_valid_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

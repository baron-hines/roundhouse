//! `delegated_type :role, types: [...]` defines what Rails 8.1's
//! `ActiveRecord::DelegatedType` defines: a polymorphic `belongs_to`
//! (a new delegate saves with its owner), `<role>_types`,
//! `<role>_class`, `<role>_name` (a string inquirer),
//! `build_<role>`, and per type a scope, a predicate, a reader and an
//! id reader; `primary_key:`, `foreign_key:` and `foreign_type:` rename
//! the columns those read. The expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/card.rb", r##"class Card < ApplicationRecord
  delegated_type :cardable, types: %w[ Note Photo Billing::Invoice ], dependent: :destroy
end
"##)
        .write("app/models/note.rb", r##"class Note < ApplicationRecord
  has_one :card, as: :cardable
end
"##)
        .write("app/models/photo.rb", r##"class Photo < ApplicationRecord
  has_one :card, as: :cardable
end
"##)
        .write("app/models/billing/invoice.rb", r##"module Billing
  class Invoice < ApplicationRecord
    self.table_name = "invoices"
  end
end
"##)
        .write("app/models/tile.rb", r##"class Tile < ApplicationRecord
  delegated_type :shape, types: %w[ Note Photo ], primary_key: :code, foreign_key: :shape_code, foreign_type: :shape_kind, optional: true

  def build_shape(attributes = {})
    self.shape = Note.new(attributes)
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.reset
    [Card, Tile, Note, Photo, Billing::Invoice].each(&:delete_all)
  end

  # Rails: [true, true, "first"]
  def self.new_delegate_saves_with_its_owner
    card = Card.create!(cardable: Note.new(body: "first"))
    [card.cardable.persisted?, card.cardable_id == card.cardable.id, Card.find(card.id).cardable.body]
  end

  # Rails: [true, "first", true, false, nil, nil]
  def self.type_readers
    note = Note.create!(body: "first")
    card = Card.find(Card.create!(cardable: note).id)
    [card.note?, card.note.body, card.note_id == note.id, card.photo?, card.photo, card.photo_id]
  end

  # Rails: ["Note", "note", true, false, "billing_invoice", true]
  def self.class_and_name
    card = Card.create!(cardable: Note.create!(body: "n"))
    bill = Card.create!(cardable: Billing::Invoice.create!(total: 5))
    [card.cardable_class.name, card.cardable_name, card.cardable_name.note?, card.cardable_name.photo?,
     bill.cardable_name, bill.billing_invoice?]
  end

  # Rails: [["Note", "Photo", "Billing::Invoice"], 2, 1, 1]
  def self.types_and_scopes
    Card.create!(cardable: Note.create!(body: "a"))
    Card.create!(cardable: Note.create!(body: "b"))
    Card.create!(cardable: Photo.create!(caption: "c"))
    Card.create!(cardable: Billing::Invoice.create!(total: 1))
    [Card.cardable_types, Card.notes.count, Card.photos.count, Card.billing_invoices.count]
  end

  # Rails: ["Photo", "built", true, 1]
  def self.build_delegate
    card = Card.new(cardable_type: "Photo")
    card.build_cardable(caption: "built")
    card.save!
    [card.cardable.class.name, card.cardable.caption, card.photo?, Photo.count]
  end

  # Rails: [1, 0]
  def self.destroy_takes_the_delegate
    card = Card.create!(cardable: Note.new(body: "gone"))
    before = Note.count
    card.destroy
    [before, Note.count]
  end

  # Rails: ["n-1", "Note", true, "n-1", "kept", nil, 1]
  def self.renamed_columns
    note = Note.create!(body: "kept", code: "n-1")
    tile = Tile.find(Tile.create!(shape: note).id)
    [tile.shape_code, tile.shape_kind, tile.note?, tile.note_code, tile.note.body, tile.photo_code, Tile.notes.count]
  end

  # Rails: ["Note", "drawn"] (the model's own builder, written after)
  def self.own_builder
    tile = Tile.new
    tile.build_shape(body: "drawn")
    [tile.shape.class.name, tile.shape.body]
  end

  def self.run
    reset
    check("own_builder", own_builder == ["Note", "drawn"])
    reset
    check("new_delegate_saves_with_its_owner", new_delegate_saves_with_its_owner == [true, true, "first"])
    reset
    check("type_readers", type_readers == [true, "first", true, false, nil, nil])
    reset
    check("class_and_name", class_and_name == ["Note", "note", true, false, "billing_invoice", true])
    reset
    check("types_and_scopes", types_and_scopes == [["Note", "Photo", "Billing::Invoice"], 2, 1, 1])
    reset
    check("build_delegate", build_delegate == ["Photo", "built", true, 1])
    reset
    check("destroy_takes_the_delegate", destroy_takes_the_delegate == [1, 0])
    reset
    check("renamed_columns", renamed_columns == ["n-1", "Note", true, "n-1", "kept", nil, 1])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "cards", force: :cascade do |t|
    t.string "cardable_type", null: false
    t.integer "cardable_id", null: false
  end
  create_table "tiles", force: :cascade do |t|
    t.string "shape_kind"
    t.string "shape_code"
  end
  create_table "notes", force: :cascade do |t|
    t.string "body"
    t.string "code"
  end
  create_table "photos", force: :cascade do |t|
    t.string "caption"
  end
  create_table "invoices", force: :cascade do |t|
    t.integer "total"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn delegated_type_defines_what_rails_defines() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn delegated_type_defines_what_rails_defines_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

/// The inquirer read in a test body, not only in the app's code.
const TEST: &str = r##"require "test_helper"

class CardTest < ActiveSupport::TestCase
  test "the role name is an inquirer" do
    card = Card.create!(cardable: Photo.create!(caption: "c"))
    assert card.cardable_name.photo?
    assert_not card.cardable_name.note?
  end
end
"##;

#[test]
fn delegated_type_role_name_is_an_inquirer_in_a_test() {
    app().write("test/models/card_test.rb", TEST).run_test("test/models/card_test.rb").assert_passes();
}

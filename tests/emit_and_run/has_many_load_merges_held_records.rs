//! A record a has_many holds before it loads (here from `create!`) stays
//! the same object once the association loads, and takes the row's value
//! of each attribute it has not changed while keeping its own unsaved
//! changes, as Rails' `merge_target_lists` merges memory with the rows.
//! A held record that is already saved is not validated or saved again
//! when its owner saves (`associated_records_to_validate_or_save` answers
//! the whole target only for a new owner). The probe's expected values
//! are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/shelf.rb", r##"class Shelf < ApplicationRecord
  has_many :books
end
"##)
        .write("app/models/book.rb", r##"class Book < ApplicationRecord
  belongs_to :shelf

  validates :title, presence: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    s = Shelf.create!(label: "fiction")
    b = s.books.create!(title: "draft", pages: 10)
    Book.where(id: b.id).update_all(pages: 250, title: "final")
    b.title = "edited"
    loaded = s.books.to_a
    # Rails: the held object, pages from the row, title kept as edited
    check("same object", loaded.length == 1 && loaded.first.equal?(b))
    check("unchanged attribute from the row", b.pages == 250)
    check("changed attribute kept", b.title == "edited")
    # Rails: a held record that is already saved is neither saved nor
    # validated with its (persisted) owner
    b.pages = 5
    s.label = "second"
    check("owner saves", s.save == true)
    check("held row not saved", Book.find(b.id).pages == 250)
    b.title = ""
    s.label = "third"
    check("owner still saves", s.save == true)
    check("held row not validated", s.valid? == true)
    # Rails: a create! on a preloaded association joins the loaded target
    # too, so the reader answers it and the count sees it once.
    p = Shelf.create!(label: "seeded")
    first = p.books.create!(title: "first", pages: 1)
    p = Shelf.where(id: p.id).preload(:books).first
    late = p.books.create!(title: "later", pages: 2)
    check("created joins the loaded target", p.books.to_a.map(&:title) == ["first", "later"])
    check("same created object", p.books.to_a.last.equal?(late))
    check("size counts it once", p.books.size == 2 && p.books.length == 2)
    check("presence sees it", p.books.any? == true && p.books.empty? == false)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "shelves", force: :cascade do |t|
    t.string "label"
  end
  create_table "books", force: :cascade do |t|
    t.string "title"
    t.integer "pages"
    t.integer "shelf_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_has_many_load_merges_the_rows_into_the_records_it_holds() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_has_many_load_merges_the_rows_into_the_records_it_holds_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

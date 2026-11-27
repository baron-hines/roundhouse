//! An association that loads records seats its owner on each of them
//! through the automatic inverse, so the record reads that very owner
//! object back, as Rails' `set_inverse_instance` does: a has_many or
//! has_one reader, a preload of either, and a belongs_to reader or
//! preload whose inverse is a has_one. An explicit `foreign_key:` on
//! either side leaves no automatic inverse, and the rows read their owner
//! afresh. The expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/library.rb", r##"class Library < ApplicationRecord
  has_many :books
  has_many :volumes, class_name: "Book", foreign_key: :library_id
  has_one :catalog
  has_many :shelves
end
"##)
        .write("app/models/book.rb", r##"class Book < ApplicationRecord
  belongs_to :library
end
"##)
        .write("app/models/shelf.rb", r##"class Shelf < ApplicationRecord
  belongs_to :library, foreign_key: :library_id
end
"##)
        .write("app/models/catalog.rb", r##"class Catalog < ApplicationRecord
  belongs_to :library
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    city = Library.create!(name: "city")
    town = Library.create!(name: "town")
    Book.create!(title: "atlas", library: city)
    Book.create!(title: "lexicon", library: city)
    Book.create!(title: "primer", library: town)
    Catalog.create!(edition: 3, library: city)
    Shelf.create!(label: "a", library: city)

    lib = Library.find(city.id)
    books = lib.books.to_a
    # Rails: each loaded book holds the library object that loaded it
    check("has_many reader", books.length == 2 && books.all? { |b| b.library.equal?(lib) })
    check("explicit foreign_key reads afresh", lib.volumes.to_a.none? { |b| b.library.equal?(lib) })
    check("explicit belongs_to foreign_key reads afresh", lib.shelves.to_a.none? { |s| s.library.equal?(lib) })

    libs = Library.order(:id).preload(:books).to_a
    check("has_many preload", libs.all? { |l| l.books.all? { |b| b.library.equal?(l) } })
    libs = Library.order(:id).includes(:books).to_a
    check("has_many includes", libs.first.books.length == 2 && libs.all? { |l| l.books.all? { |b| b.library.equal?(l) } })

    lib = Library.find(city.id)
    check("has_one reader", lib.catalog.library.equal?(lib))
    libs = Library.order(:id).preload(:catalog).to_a
    check("has_one preload", libs.first.catalog.library.equal?(libs.first) && libs.last.catalog.nil?)

    cat = Catalog.first
    # Rails: the library the catalog loads answers that catalog as its has_one
    check("belongs_to reader", cat.library.catalog.equal?(cat))
    cats = Catalog.preload(:library).to_a
    check("belongs_to preload", cats.first.library.catalog.equal?(cats.first))
    book = Book.find_by(title: "primer")
    check("belongs_to with a has_many inverse", book.library.name == "town" && book.library.books.to_a.none? { |b| b.equal?(book) })
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
  create_table "shelves", force: :cascade do |t|
    t.string "label"
    t.integer "library_id"
  end
  create_table "catalogs", force: :cascade do |t|
    t.integer "edition"
    t.integer "library_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn an_association_load_seats_its_owner_through_the_inverse() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_association_load_seats_its_owner_through_the_inverse_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

//! Under a scope of its model, `Model.all` is the whole scope, its limit,
//! offset, select and preload as well as its conditions and order, as
//! Rails' `all` answers `scope.clone`. A class method called through a
//! relation runs under that relation's scope, so its `all` and `where`
//! start from everything the relation holds. An association's relation
//! ignores the current scope (Rails' `scope_for_association`), so a class
//! method reached through an association reads the owner's rows.
//! The probe's expected values are ActiveRecord 8.1.4's answers.

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
  belongs_to :library

  def self.titles
    all.map(&:title)
  end

  def self.on_shelf
    where.not(title: "Missing").map(&:title)
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, actual, expected)
    raise "#{label}: #{actual.inspect}, Rails answers #{expected.inspect}" unless actual == expected
  end

  def self.run
    library = Library.create!(name: "branch")
    %w[Dune Emma Ivanhoe Ulysses].each { |title| Book.create!(library: library, title: title) }
    check("class method through a relation", Book.order(:id).limit(2).offset(1).titles, ["Emma", "Ivanhoe"])
    check("where in a class method", Book.order(:id).offset(3).on_shelf, ["Ulysses"])
    check("scoping limit", Book.order(:id).limit(1).scoping { Book.all.map(&:title) }, ["Dune"])
    check("scoping offset", Book.order(:id).offset(3).scoping { Book.where.not(title: "Missing").map(&:title) }, ["Ulysses"])
    check("scoping select", Book.select(:id, :title).scoping { Book.all.first.has_attribute?(:library_id) }, false)
    check("scoping preload", Library.preload(:books).scoping { Library.all.first.association(:books).loaded? }, true)
    check("scoping where", Book.where(title: "Emma").scoping { Book.all.map(&:title) }, ["Emma"])
    check("class-level count under scoping", Book.where(title: "Emma").scoping { Book.count }, 1)
    check("association under scoping", Book.where(title: "Emma").scoping { library.books.on_shelf }, ["Dune", "Emma", "Ivanhoe", "Ulysses"])
    shelf = Book.order(:id)
    check("scoping reorder merged", Book.reorder(title: :desc).scoping { shelf.merge(Book.all).map(&:title) }, ["Ulysses", "Ivanhoe", "Emma", "Dune"])
    check("restored after", Book.all.map(&:title), ["Dune", "Emma", "Ivanhoe", "Ulysses"])
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
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn all_under_a_scope_is_the_whole_scope() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn all_under_a_scope_is_the_whole_scope_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

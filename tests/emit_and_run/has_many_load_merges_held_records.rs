//! A record a has_many holds before it loads (here from `create!`) stays
//! the same object once the association loads, and takes the row's value
//! of each attribute it has not changed while keeping its own unsaved
//! changes, as Rails' `merge_target_lists` merges memory with the rows.
//! Ignored as a fork spec until the compiler does; the expected values
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
#[ignore = "fork spec: association targets"]
fn a_has_many_load_merges_the_rows_into_the_records_it_holds() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: association targets"]
fn a_has_many_load_merges_the_rows_into_the_records_it_holds_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

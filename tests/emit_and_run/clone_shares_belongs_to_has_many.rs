//! A record and its clone share the belongs_to target and the has_many
//! rows either of them loads, so the other reads them without a query,
//! as Rails' clone keeps the original's association cache. Ignored as a
//! fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/author.rb", r##"class Author < ApplicationRecord
end
"##)
        .write("app/models/book.rb", r##"class Book < ApplicationRecord
  belongs_to :author, optional: true
  has_many :chapters
end
"##)
        .write("app/models/chapter.rb", r##"class Chapter < ApplicationRecord
  belongs_to :book, optional: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.queries
    count = 0
    counter = ->(event) { count += 1 unless event.payload[:name] == "SCHEMA" }
    ActiveSupport::Notifications.subscribed(counter, "sql.active_record") { yield }
    count
  end

  def self.run
    a = Author.create!(name: "ann")
    b = Book.create!(title: "dune", author_id: a.id)
    Chapter.create!(book_id: b.id, title: "one")
    Chapter.create!(book_id: b.id, title: "two")
    original = Book.find(b.id)
    copy = original.clone
    # Rails: 1, then 0 for the original
    check("the clone loads the belongs_to", queries { copy.author.name } == 1)
    check("the original reads the clone's belongs_to", queries { original.author.name } == 0)
    check("the clone loads the has_many", queries { copy.chapters.to_a.size } == 1)
    check("the original reads the clone's has_many", queries { original.chapters.map(&:title) } == 0)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "authors", force: :cascade do |t|
    t.string "name"
  end
  create_table "books", force: :cascade do |t|
    t.string "title"
    t.integer "author_id"
  end
  create_table "chapters", force: :cascade do |t|
    t.integer "book_id"
    t.string "title"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: persistence"]
fn a_record_and_its_clone_share_belongs_to_and_has_many() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: persistence"]
fn a_record_and_its_clone_share_belongs_to_and_has_many_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

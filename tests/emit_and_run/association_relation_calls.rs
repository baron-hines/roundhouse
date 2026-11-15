//! Relation calls through a has_many: `sum(:col)` sums the saved rows
//! (not the held unsaved ones), `from(...)` keeps the association's key,
//! and `extract_associated(:name)` preloads and maps the association.
//! The probe's expected values are ActiveRecord 8.1.4's answers (the sums
//! compare equal, though ours are Floats where Rails' are Integers: the
//! runtime's `Relation#sum`).

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/author.rb", r##"class Author < ApplicationRecord
  has_many :posts
end
"##)
        .write("app/models/post.rb", r##"class Post < ApplicationRecord
  belongs_to :author, optional: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.reset
    [Post, Author].each(&:delete_all)
  end

  # Rails: [10, 10, 10]
  def self.sum_on_association_ignores_unsaved
    a = Author.create!(name: "a"); Post.create!(title: "1", author: a, words: 10); a.posts.build(title: "2", words: 7)
    [a.posts.sum(:words), Post.where(author_id: a.id).sum(:words), Post.sum(:words)]
  end

  # Rails: ["t"]
  def self.from_on_has_many
    a = Author.create!(name: "a"); Post.create!(title: "t", author: a)
    a.posts.from("posts").map(&:title)
  end

  # Rails: [["1", "3"], ["2"]]
  def self.extract_associated
    a = Author.create!(name: "a"); b = Author.create!(name: "b"); Post.create!(title: "1", author: a); Post.create!(title: "2", author: b); Post.create!(title: "3", author: a)
    Author.order(:id).extract_associated(:posts).map { |ps| ps.map(&:title).sort }
  end

  # Rails: [["a", "a"], ["b"]]
  def self.extract_associated_through_a_has_many
    a = Author.create!(name: "a"); b = Author.create!(name: "b"); Post.create!(title: "1", author: a); Post.create!(title: "3", author: a)
    Post.create!(title: "2", author: b)
    [a.posts.extract_associated(:author).map(&:name), b.posts.extract_associated(:author).map(&:name)]
  end

  def self.run
    reset
    check("sum_on_association_ignores_unsaved", sum_on_association_ignores_unsaved == [10, 10, 10])
    reset
    check("from_on_has_many", from_on_has_many == ["t"])
    reset
    check("extract_associated", extract_associated == [["1", "3"], ["2"]])
    reset
    check("extract_associated_through_a_has_many", extract_associated_through_a_has_many == [["a", "a"], ["b"]])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "authors", force: :cascade do |t|
    t.string "name"
    t.boolean "active", default: true
  end
  create_table "posts", force: :cascade do |t|
    t.string "title"
    t.integer "author_id"
    t.boolean "published", default: false
    t.integer "words", default: 0
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn relation_calls_through_a_has_many_answer_as_rails_does() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn relation_calls_through_a_has_many_answer_as_rails_does_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

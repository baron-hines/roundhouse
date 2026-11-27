//! A has_many's target includes the records it holds unsaved (`build`,
//! `<<` on a new owner): `first`/`last` see them after the saved rows,
//! `size` counts them on top of the `COUNT`, `destroy_all` destroys them
//! too, and `reload` drops them, as Rails 8.1.4's `CollectionAssociation`
//! does (`ingest::has_many_hold`, routed by `lower::scope_chain`). The
//! probe's expected values are ActiveRecord 8.1.4's answers.

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

  # Rails: ["1", "2"]
  def self.has_many_first_last_new_owner
    a = Author.new(name: "a"); p1 = a.posts.build(title: "1"); p2 = a.posts.build(title: "2")
    [a.posts.first&.title, a.posts.last&.title]
  end

  # Rails: ["s", "u"]
  def self.has_many_first_saved_owner_unsaved_record
    a = Author.create!(name: "a"); Post.create!(title: "s", author: a); a.posts.build(title: "u")
    [a.posts.first&.title, a.posts.last&.title]
  end

  # Rails: [1, 2, "x", nil, nil]
  def self.count_find_by_through_proxy
    a = Author.create!(name: "a"); Post.create!(title: "x", author: a); Post.create!(title: "y"); a.posts.build(title: "z")
    [a.posts.count, a.posts.size, a.posts.find_by(title: "x")&.title, a.posts.find_by(title: "y"), a.posts.find_by(title: "z")]
  end

  # Rails: [2, 0, 0, 0]
  def self.destroy_all_on_has_many
    a = Author.create!(name: "a"); Post.create!(title: "1", author: a); a.posts.load; a.posts.build(title: "b")
    d = a.posts.destroy_all
    [d.size, a.posts.size, a.posts.to_a.size, Post.count]
  end

  # Rails: [0, 0]
  def self.reload_drops_unloaded_target
    a = Author.create!(name: "a"); a.posts.build(title: "u"); a.reload
    [a.posts.size, a.posts.to_a.size]
  end

  def self.run
    reset
    check("has_many_first_last_new_owner", has_many_first_last_new_owner == ["1", "2"])
    reset
    check("has_many_first_saved_owner_unsaved_record", has_many_first_saved_owner_unsaved_record == ["s", "u"])
    reset
    check("count_find_by_through_proxy", count_find_by_through_proxy == [1, 2, "x", nil, nil])
    reset
    check("destroy_all_on_has_many", destroy_all_on_has_many == [2, 0, 0, 0])
    reset
    check("reload_drops_unloaded_target", reload_drops_unloaded_target == [0, 0])
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
fn a_has_many_target_includes_held_records_as_rails_does() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_has_many_target_includes_held_records_as_rails_does_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

//! `relation.except(*clauses)` drops the named query clauses and keeps
//! the rest, and `relation.only(*clauses)` keeps the named ones and drops
//! the rest, as Rails' SpawnMethods answer them. Putting featured posts
//! ahead of a scope's own order reads
//! `scope.except(:order).order(featured: :desc).merge(scope.only(:order))`.
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/post.rb", r##"class Post < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, actual, expected)
    raise "#{label}: #{actual.inspect}, Rails answers #{expected.inspect}" unless actual == expected
  end

  def self.run
    [["walrus", 1, 0], ["badger", 1, 1], ["otter", 1, 1], ["aardvark", 1, 0], ["zebra", 2, 0]].each do |title, author, featured|
      Post.create!(title: title, author_id: author, featured: featured)
    end
    scope = Post.where(author_id: 1).order(:title).limit(3)
    check("except order", scope.except(:order).order(title: :desc).map(&:title), ["walrus", "otter", "badger"])
    check("except where and limit", scope.except(:where, :limit).map(&:title), ["aardvark", "badger", "otter", "walrus", "zebra"])
    check("except limit", scope.except(:limit).count, 4)
    check("only order", scope.only(:order).map(&:title), ["aardvark", "badger", "otter", "walrus", "zebra"])
    check("only where and limit", scope.only(:where, :limit).map(&:title), ["walrus", "badger", "otter"])
    check("only nothing", Post.where(author_id: 2).distinct.only.count, 5)
    featured_first = scope.except(:order).order(featured: :desc).merge(scope.only(:order))
    check("featured first", featured_first.map(&:title), ["badger", "otter", "aardvark"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "posts", force: :cascade do |t|
    t.string "title"
    t.integer "author_id"
    t.integer "featured", default: 0
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn except_and_only_keep_the_clauses_rails_keeps() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn except_and_only_keep_the_clauses_rails_keeps_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

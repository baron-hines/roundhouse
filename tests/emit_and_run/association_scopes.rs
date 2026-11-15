//! An association's scope applies wherever it's read: a has_many lambda
//! that calls a named scope (`-> { long }`), a scoped `belongs_to`'s
//! reader and its preload (which keeps the lazy, scoped reader), and a
//! `through:` hop whose association is scoped (`where(<hop table>: ...)`).
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;
use roundhouse::build_target::BuildTarget;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/author.rb", r##"class Author < ApplicationRecord
  has_many :posts
  has_many :published_posts, -> { where(published: true) }, class_name: "Post"
  has_many :long_posts, -> { long }, class_name: "Post"
  has_many :published_comments, through: :published_posts, source: :comments
  has_many :published_post_writers, through: :published_posts, source: :author
end
"##)
        .write("app/models/comment.rb", r##"class Comment < ApplicationRecord
  belongs_to :post
end
"##)
        .write("app/models/post.rb", r##"class Post < ApplicationRecord
  belongs_to :author, optional: true
  belongs_to :live_author, -> { where(active: true) }, class_name: "Author", foreign_key: :author_id, optional: true
  belongs_to :renamed_author, -> { where(name: "a").where(name: "b") }, class_name: "Author", foreign_key: :author_id, optional: true
  belongs_to :other_author, -> { where(id: 0) }, class_name: "Author", foreign_key: :author_id, optional: true
  has_many :comments
  scope :long, -> { where("words > 100") }
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.reset
    [Comment, Post, Author].each(&:delete_all)
  end

  # Rails: [["p"], 1]
  def self.has_many_scope_where
    a = Author.create!(name: "a"); Post.create!(title: "p", author: a, published: true); Post.create!(title: "d", author: a)
    [a.published_posts.map(&:title), a.published_posts.count]
  end

  # Rails: [["l"], 1]
  def self.has_many_named_scope
    a = Author.create!(name: "a"); Post.create!(title: "l", author: a, words: 500); Post.create!(title: "s", author: a, words: 5)
    [a.long_posts.map(&:title), a.long_posts.count]
  end

  # Rails: [nil, "a"]
  def self.belongs_to_scope_reader
    a = Author.create!(name: "a", active: false); p = Post.create!(title: "t", author: a)
    [Post.find(p.id).live_author, Post.find(p.id).author.name]
  end

  # Rails: [nil, "b"]
  def self.preload_scoped_belongs_to
    a = Author.create!(name: "a", active: false); b = Author.create!(name: "b"); Post.create!(title: "1", author: a); Post.create!(title: "2", author: b)
    Post.preload(:live_author).order(:id).map { |p| p.live_author&.name }
  end

  # Rails: [["in"], 1]
  def self.through_scoped_hop
    a = Author.create!(name: "a"); pp = Post.create!(title: "p", author: a, published: true); dd = Post.create!(title: "d", author: a)
    Comment.create!(post: pp, body: "in"); Comment.create!(post: dd, body: "out")
    [a.published_comments.map(&:body), a.published_comments.count]
  end

  # Rails: [1, 1]
  def self.preload_through_scoped_hop
    a = Author.create!(name: "a"); Post.create!(title: "p", author: a, published: true); Post.create!(title: "d", author: a)
    [a.published_post_writers.to_a.size, Author.includes(:published_post_writers).find(a.id).published_post_writers.size]
  end

  # Rails: [true, true, "a"]
  def self.belongs_to_scope_repeats_a_column
    a = Author.create!(name: "a"); p = Post.create!(title: "p", author: a); p = Post.find(p.id)
    [p.renamed_author.nil?, p.other_author.nil?, p.author.name]
  end

  def self.run
    reset
    check("has_many_scope_where", has_many_scope_where == [["p"], 1])
    reset
    check("has_many_named_scope", has_many_named_scope == [["l"], 1])
    reset
    check("belongs_to_scope_reader", belongs_to_scope_reader == [nil, "a"])
    reset
    check("preload_scoped_belongs_to", preload_scoped_belongs_to == [nil, "b"])
    reset
    check("through_scoped_hop", through_scoped_hop == [["in"], 1])
    reset
    check("preload_through_scoped_hop", preload_through_scoped_hop == [1, 1])
    reset
    check("belongs_to_scope_repeats_a_column", belongs_to_scope_repeats_a_column == [true, true, "a"])
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
  create_table "comments", force: :cascade do |t|
    t.integer "post_id"
    t.string "body"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn an_association_scope_applies_as_rails_does() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_association_scope_applies_as_rails_does_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

/// A strict target has no runtime Relation. A `belongs_to` scope of hash
/// `where`s on distinct columns joins the key in `find_by`, which folds to
/// SQL there too; any other scope is the reported gap, and the reader
/// finds by the key alone.
#[test]
fn a_strict_target_folds_a_hash_belongs_to_scope_and_reports_any_other() {
    let unapplied = |overlay: emit_and_run::Overlay| {
        overlay
            .emit_warnings(BuildTarget::Rust)
            .iter()
            .any(|d| d.message.contains("`belongs_to :live_author`'s scope is not applied"))
    };
    assert!(!unapplied(app()), "a hash scope folds");
    let string_scope = app().edit("app/models/post.rb", "-> { where(active: true) }", "-> { where(\"active = 1\") }");
    assert!(unapplied(string_scope), "a string scope is reported");
    // A repeated column would be one keyword argument where Rails applies
    // both conditions, so it doesn't fold either.
    let repeated = app().edit("app/models/post.rb", "-> { where(active: true) }", "-> { where(active: true).where(active: false) }");
    assert!(unapplied(repeated), "a repeated column is reported");
}

/// A scope lambda that takes the owner, through a parameter of any kind,
/// isn't applied (the owner isn't threaded into the reader), and says so,
/// rather than keep a body that reads an unbound name.
#[test]
fn a_scope_that_takes_the_owner_is_reported_not_kept() {
    let warnings = app()
        .edit(
            "app/models/author.rb",
            "  has_many :posts\n",
            "  has_many :posts\n  has_many :titled_posts, ->(owner = nil) { where(title: owner.name) }, class_name: \"Post\"\n",
        )
        .edit(
            "app/models/post.rb",
            "  has_many :comments\n",
            "  has_many :comments\n  belongs_to :titled_author, ->(post) { where(name: post.title) }, class_name: \"Author\", foreign_key: :author_id, optional: true\n",
        )
        .ingest_warnings();
    for declared in ["has_many :titled_posts", "belongs_to :titled_author"] {
        assert!(
            warnings.iter().any(|w| w.contains(declared) && w.contains("takes the owner is not applied")),
            "{declared}: {warnings:#?}"
        );
    }
}

/// A scope body the readers can't graft onto their query, anything but a
/// chain of query calls (`-> { where(active: true) if flag }`), isn't
/// applied either, and says so, rather than read the target unscoped
/// with no word. (The body is still analyzed, so an unsupported construct
/// in it stays an error.)
#[test]
fn a_scope_that_is_not_a_query_chain_is_reported() {
    let warnings = app()
        .edit(
            "app/models/author.rb",
            "  has_many :posts\n",
            "  has_many :posts\n  has_many :maybe_posts, -> { where(title: \"t\") if true }, class_name: \"Post\"\n",
        )
        .edit(
            "app/models/post.rb",
            "  has_many :comments\n",
            "  has_many :comments\n  belongs_to :maybe_author, -> { where(name: \"a\") if true }, class_name: \"Author\", foreign_key: :author_id, optional: true\n",
        )
        .ingest_warnings();
    for declared in ["has_many :maybe_posts", "belongs_to :maybe_author"] {
        assert!(
            warnings.iter().any(|w| w.contains(declared) && w.contains("isn't a chain of query calls")),
            "{declared}: {warnings:#?}"
        );
    }
    // The probe's own scopes, all query chains, are kept without a word.
    assert!(!app().ingest_warnings().iter().any(|w| w.contains("not applied")), "{:#?}", app().ingest_warnings());
}

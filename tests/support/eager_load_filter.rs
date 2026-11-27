//! The eager-load contract shared by the Ruby lane and the Spinel lane.

use super::emit_and_run;

/// `includes(:comments)` with a condition and an order on the comments
/// table is Rails' eager load: the articles are those with a matching
/// comment, in the chain's order, and each article's `comments` holds
/// only the matching rows in that order (lobsters' filters page reads
/// `Category.includes(:tags).where(tags: …).order(…)`). The answers are
/// Rails 8.1's for the same rows.
pub const METHODS: &str = r#"  validates :title, presence: true

  def self.with_comments_by(name)
    includes(:comments, comments: :article)
      .where(comments: { commenter: name })
      .where(comments: { body: ["one", "three", "four", "five"] })
      .where.not(comments: { body: "one" })
      .order([articles: { title: :asc }, comments: { body: :desc }])
  end
"#;

pub const ASSERTIONS: &str = r#"Comment.delete_all
Article.delete_all
b = Article.create!(title: "Beta", body: "A sufficiently long body.")
a = Article.create!(title: "Alpha", body: "A sufficiently long body.")
c = Article.create!(title: "Gamma", body: "A sufficiently long body.")
Comment.create!(article: b, commenter: "Ann", body: "one")
Comment.create!(article: b, commenter: "Bob", body: "two")
Comment.create!(article: b, commenter: "Ann", body: "three")
Comment.create!(article: b, commenter: "Bob", body: "four")
Comment.create!(article: a, commenter: "Ann", body: "four")
Comment.create!(article: c, commenter: "Bob", body: "five")
got = Article.with_comments_by("Ann").to_a.map { |art| [art.title, art.comments.map { |cm| cm.body + "@" + cm.article.title }] }
want = [["Alpha", ["four@Alpha"]], ["Beta", ["three@Beta"]]]
raise "eager: #{got.inspect}" unless got == want
puts "eager load passed"
"#;

#[allow(dead_code)]
pub fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit("app/models/article.rb", "  validates :title, presence: true\n", METHODS)
}


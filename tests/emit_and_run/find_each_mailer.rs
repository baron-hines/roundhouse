//! A record from `find_each` (from the class, a relation and a has_many
//! reader) and an Array of records from `find_in_batches` reach a mailer
//! typed: `check` is clean, and the emitted program delivers the mail.
//! The block parameters were unbound, and `check` reported
//! `@article has no known type` in the mailer's views.

use super::emit_and_run;

const MAILER: &str = r#"class DigestMailer < ApplicationMailer
  def article(article)
    @article = article
    mail(to: "reader@example.com", subject: "Article")
  end

  def comment(comment)
    @comment = comment
    mail(to: "reader@example.com", subject: "Comment")
  end

  def batch(articles)
    @articles = articles
    mail(to: "reader@example.com", subject: "Batch")
  end
end
"#;

const ANNOUNCE: &str = r#"  validates :title, presence: true

  def announce
    Article.find_each { |article| DigestMailer.article(article).deliver_now }
    Article.where(title: title).find_each { |article| DigestMailer.article(article).deliver_now }
    comments.find_each { |comment| DigestMailer.comment(comment).deliver_now }
    Article.find_in_batches { |articles| DigestMailer.batch(articles).deliver_now }
  end
"#;

const ASSERTIONS: &str = r#"
one = Article.create!(title: "One", body: "A body long enough for validation.")
Article.create!(title: "Two", body: "Another body long enough for validation.")
Comment.create!(article: one, commenter: "x", body: "First!")
one.announce
got = ActionMailer::Base.deliveries.map { |m| m.body.to_s.strip }
raise "delivered: #{got.inspect}" unless got == ["Article: One", "Article: Two", "Article: One", "Comment: First!", "Batch: One, Two"]
puts "batch block params passed"
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/mailers/digest_mailer.rb", MAILER)
        .write("app/views/digest_mailer/article.text.erb", "Article: <%= @article.title %>\n")
        .write("app/views/digest_mailer/comment.text.erb", "Comment: <%= @comment.body %>\n")
        .write("app/views/digest_mailer/batch.text.erb", "Batch: <%= @articles.map(&:title).join(\", \") %>\n")
        .edit("app/models/article.rb", "  validates :title, presence: true\n", ANNOUNCE)
}

#[test]
fn check_types_what_the_batch_iterators_yield() {
    let errors = app().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn a_record_from_find_each_reaches_the_mailer() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("batch block params passed"), "{}", run.stdout);
}

//! lobsters' `Search` builds its query in place and from a parse tree:
//! `query.joins!(:user).where!(users: { username: … })`,
//! `query.where!(story: Story.joins(:tags).where(tags: tags))`,
//! `where!(story: { users: { username: … } })` after `joins!(story: :user)`,
//! and `value.pluck(:term)` on a parse-tree node. `joins!` / `where!` were
//! not read as `joins` / `where`, so the association join raised and
//! `story:` became a `comments.story` column; a has_many key with a
//! Relation became `stories.tags`; the nested hash became `story.users`;
//! and the untyped `pluck` had no Array method behind it.

use super::emit_and_run;

const FINDER: &str = r##"class CommentFinder
  def self.on_title(title)
    query = Comment.all
    query.joins!(:article).where!(articles: { title: title })
    query
  end

  def self.on_title_split(title)
    query = Comment.all
    query.joins!(:article)
    query.where!(article: { title: title })
    query
  end

  def self.column(source)
    source.pluck(:commenter)
  end

  def self.on_articles(articles)
    query = Comment.all
    query.where!(article: articles)
    query
  end

  def self.articles_with(comments)
    Article.joins(:comments).where(comments: comments).distinct
  end

  def self.nested(name)
    Comment.joins(article: :comments).where(article: { comments: { commenter: name } })
  end

  def self.terms(tree)
    tree.pluck(:term)
  end
end
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/comment_finder.rb", FINDER)
}

const ASSERTIONS: &str = r##"first = Article.create!(title: "First", body: "a body that is long enough")
second = Article.create!(title: "Second", body: "a body that is long enough")
a = Comment.create!(article: first, commenter: "ann", body: "one")
b = Comment.create!(article: second, commenter: "bob", body: "two")
got = [
  CommentFinder.on_title("Second").map(&:id),
  CommentFinder.on_articles(Article.where(title: "First")).map(&:id),
  CommentFinder.articles_with(Comment.where(commenter: "bob")).map(&:id),
  CommentFinder.nested("ann").to_sql.include?("\"comments\".\"commenter\" = 'ann'"),
  CommentFinder.terms([{ term: "exact" }, { term: "phrase" }]),
  CommentFinder.on_title_split("Second").map(&:id),
  CommentFinder.column(Comment).sort,
  CommentFinder.column([{ commenter: "x" }])
]
raise got.inspect unless got == [[b.id], [a.id], [second.id], true, ["exact", "phrase"], [b.id], ["ann", "bob"], ["x"]]
puts "bang relation builders passed"
"##;

#[test]
fn bang_relation_builders_run() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("bang relation builders passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn bang_relation_builders_run_on_spinel() {
    let run = app().run_spinel_with_db(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("bang relation builders passed"), "{}", run.stdout);
}

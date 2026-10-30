//! `render @posts, flag: "no"` in a view: for any first argument but a
//! Hash, Rails renders the object's partial with the hash as its
//! locals, once per record for a collection, and every key is a local
//! (`as:` included). The same calls without the hash already rendered;
//! with it they stayed a bare `render`. A strict-locals partial takes
//! the locals as keywords, from this form and from an explicit
//! `collection:` with `locals:` alike.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;

const ACTIONS: &[(&str, &str, &str)] = &[
    ("listed", "@posts = Post.order(:title)", "<%= render @posts, flag: \"no\" %>"),
    ("by_author", "@author = Author.first\n    @marker = \"*\"", "<%= render @author.posts, flag: @marker %>"),
    ("by_title", "@author = Author.first", "<%= render @author.posts.order(:title), flag: \"yes\" %>"),
    ("first", "@post = Post.order(:title).first", "<%= render @post, flag: \"one\" %>"),
    ("none", "@posts = Post.where(title: \"none\")", "<%= render(@posts, flag: \"no\") || \"empty\" %>"),
    ("filled", "@posts = Post.order(:title)", "<%= render(@posts, flag: \"no\") || \"empty\" %>"),
    ("as_local", "@posts = Post.order(:title)", "<%= render @posts, flag: \"as\", as: :item %>"),
    ("comments", "@comments = Comment.order(:body)", "<%= render @comments %>"),
    ("articles", "@articles = Article.order(:title)", "<%= render @articles, mark: \"+\" %>"),
    ("article", "@article = Article.order(:title).first", "<%= render @article, mark: \"+\" %>"),
    (
        "named_articles",
        "@articles = Article.order(:title)",
        "<%= render partial: \"articles/article\", collection: @articles, locals: { mark: \"!\" } %>",
    ),
    ("plain_articles", "@articles = Article.order(:title)", "<%= render @articles %>"),
];

fn app() -> emit_and_run::Overlay {
    let mut routes = String::from("Rails.application.routes.draw do\n");
    let mut controller = String::from("class AuthorsController < ApplicationController\n");
    let mut overlay = emit_and_run::empty_app();
    for (action, body, render) in ACTIONS {
        routes.push_str(&format!("  get \"/{action}\", to: \"authors#{action}\"\n"));
        controller.push_str(&format!("  def {action}\n    {body}\n  end\n\n"));
        overlay = overlay.write(&format!("app/views/authors/{action}.html.erb"), &format!("<ul>{render}</ul>\n"));
    }
    routes.push_str("end\n");
    controller.push_str("end\n");
    overlay
        .write("config/routes.rb", &routes)
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("app/controllers/authors_controller.rb", &controller)
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema[8.1].define(version: 2026_01_01_000000) do\n  create_table \"authors\", force: :cascade do |t|\n    t.string \"name\"\n  end\n  create_table \"posts\", force: :cascade do |t|\n    t.string \"title\"\n    t.bigint \"author_id\"\n  end\n  create_table \"comments\", force: :cascade do |t|\n    t.string \"body\"\n    t.bigint \"author_id\"\n  end\n  create_table \"articles\", force: :cascade do |t|\n    t.string \"title\"\n  end\nend\n",
        )
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/author.rb", "class Author < ApplicationRecord\n  has_many :posts\n  has_many :comments\nend\n")
        .write("app/models/post.rb", "class Post < ApplicationRecord\n  belongs_to :author\nend\n")
        .write("app/models/comment.rb", "class Comment < ApplicationRecord\n  belongs_to :author\nend\n")
        .write("app/views/posts/_post.html.erb", "<li><%= post.title %>:<%= flag %></li>\n")
        .write("app/views/comments/_comment.html.erb", "<li><%= comment.body %></li>\n")
        .write("app/models/article.rb", "class Article < ApplicationRecord\nend\n")
        .write(
            "app/views/articles/_article.html.erb",
            "<%# locals: (article:, mark: \"-\") -%>\n<li><%= mark %><%= article.title %></li>\n",
        )
}

const SEED: &str = r#"author = Author.create!(name: "Ann")
Post.create!(title: "b", author_id: author.id)
Post.create!(title: "a", author_id: author.id)
Post.create!(title: "c", author_id: author.id)
Comment.create!(body: "y", author_id: author.id)
Comment.create!(body: "x", author_id: author.id)
Article.create!(title: "q")
Article.create!(title: "p")
"#;

/// What Rails renders for each action, in `ACTIONS` order.
const WANT: &str = r#"want = {
  "listed" => "<ul><li>a:no</li>\n<li>b:no</li>\n<li>c:no</li>\n</ul>\n",
  "by_author" => "<ul><li>b:*</li>\n<li>a:*</li>\n<li>c:*</li>\n</ul>\n",
  "by_title" => "<ul><li>a:yes</li>\n<li>b:yes</li>\n<li>c:yes</li>\n</ul>\n",
  "first" => "<ul><li>a:one</li>\n</ul>\n",
  "none" => "<ul>empty</ul>\n",
  "filled" => "<ul><li>a:no</li>\n<li>b:no</li>\n<li>c:no</li>\n</ul>\n",
  "as_local" => "<ul><li>a:as</li>\n<li>b:as</li>\n<li>c:as</li>\n</ul>\n",
  "comments" => "<ul><li>x</li>\n<li>y</li>\n</ul>\n",
  "articles" => "<ul><li>+p</li>\n<li>+q</li>\n</ul>\n",
  "article" => "<ul><li>+p</li>\n</ul>\n",
  "named_articles" => "<ul><li>!p</li>\n<li>!q</li>\n</ul>\n",
  "plain_articles" => "<ul><li>-p</li>\n<li>-q</li>\n</ul>\n",
}
"#;

#[test]
fn an_object_render_passes_its_hash_as_the_partials_locals_on_ruby() {
    let script = format!(
        r##"{SEED}{WANT}got = want.keys.to_h do |action|
  status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/#{{action}}", "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
  [action, "#{{status}} #{{body.respond_to?(:join) ? body.join : body}}"]
end
want.each do |action, html|
  raise "#{{action}}: #{{got[action].inspect}}" unless got[action] == "200 #{{html}}"
end
puts "object renders with locals passed"
"##
    );
    let run = app().run_ruby(&script);
    run.assert_passes();
    assert!(run.stdout.contains("object renders with locals passed"), "{}", run.stdout);
}

#[test]
fn an_object_render_with_locals_emits_the_partial_call() {
    let (emitted, errors) = app().emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let view = |name: &str| std::fs::read_to_string(emitted.join(format!("app/views/authors/{name}.rb"))).unwrap();
    assert!(
        view("listed").contains("      posts.each { |p| Views::Posts.post_into(io, p, notice, alert, nil, \"no\") }\n"),
        "{}",
        view("listed")
    );
    assert!(
        view("first").contains("      Views::Posts.post_into(io, post, notice, alert, nil, \"one\")\n"),
        "{}",
        view("first")
    );
    assert!(
        view("comments").contains("      comments.each { |c| Views::Comments.comment_into(io, c, notice, alert) }\n"),
        "{}",
        view("comments")
    );
    assert!(
        view("articles").contains("      articles.each { |a| io << Views::Articles.article(a, mark: \"+\") }\n"),
        "{}",
        view("articles")
    );
    assert!(
        view("plain_articles").contains("      articles.each { |a| io << Views::Articles.article(a) }\n"),
        "{}",
        view("plain_articles")
    );
}

#[test]
fn a_render_with_a_hash_that_is_not_a_literal_stays_bare() {
    let (emitted, errors) = app()
        .write("app/views/authors/listed.html.erb", "<% opts = { flag: \"no\" } %><ul><%= render @posts, opts %></ul>\n")
        .write("app/views/authors/first.html.erb", "<ul><%= render @post, \"flag\" => \"one\" %></ul>\n")
        .emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let view = |name: &str| std::fs::read_to_string(emitted.join(format!("app/views/authors/{name}.rb"))).unwrap();
    assert!(view("listed").contains("(render posts, opts)"), "{}", view("listed"));
    assert!(view("first").contains("(render post, \"flag\" => \"one\")"), "{}", view("first"));
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_object_render_passes_its_hash_as_the_partials_locals_natively() {
    let script = format!(
        r##"{SEED}{WANT}empty = ActiveRecord::Relation.new(Post).where(title: "none")
by_title = ActiveRecord::Relation.new(Post).order(:title)
articles = ActiveRecord::Relation.new(Article).order(:title)
got = {{
  "listed" => Views::Authors.listed(by_title),
  "by_author" => Views::Authors.by_author(author, "*"),
  "by_title" => Views::Authors.by_title(author),
  "first" => Views::Authors.first(by_title.first),
  "none" => Views::Authors.none(empty),
  "filled" => Views::Authors.filled(by_title),
  "as_local" => Views::Authors.as_local(by_title),
  "comments" => Views::Authors.comments(ActiveRecord::Relation.new(Comment).order(:body)),
  "articles" => Views::Authors.articles(articles),
  "article" => Views::Authors.article(articles.first),
  "named_articles" => Views::Authors.named_articles(articles),
  "plain_articles" => Views::Authors.plain_articles(articles),
}}
want.each do |action, html|
  raise "#{{action}}: #{{got[action].inspect}}" unless got[action] == html
end
puts "object renders with locals passed"
"##
    );
    let run = app().run_spinel_with_db(&script);
    run.assert_passes();
    assert!(run.stdout.contains("object renders with locals passed"), "{}", run.stdout);
}

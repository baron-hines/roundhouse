//! `direct` route helpers called from model methods through
//! `Rails.application.routes.url_helpers`: the block is given the record,
//! its `route_for` fills each segment with `to_param` and renders the
//! caller's query key and `anchor:` (a nil query value left out), and an
//! interpolated `route_for` name calls the helper of that name; a model
//! that renders its own `to_param` fills the segment with it. The blog
//! probe (`direct_route_records`) covers tests, controllers and views;
//! this one runs on Spinel too. Its expected values are Rails 8.1.4's, from the
//! same models and routes with `default_url_options[:host]` set, which
//! a `direct` helper needs outside a request there.

use super::emit_and_run;

fn models() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/controllers/application_controller.rb", r##"class ApplicationController < ActionController::Base
end
"##)
        .write("app/controllers/articles_controller.rb", r##"class ArticlesController < ApplicationController
  def show
    render plain: "ok"
  end
end
"##)
        .write("config/routes.rb", r##"Rails.application.routes.draw do
  resources :articles, only: :show do
    resources :comments, only: :show
  end

  get "/read/:id/:slug", to: "articles#show", constraints: { id: /\d+/ }, as: :slugged_article
  get "/topics/:id", to: "articles#show", as: :forum_thread
  get "/archive(/:year)", to: "articles#show", as: :yearly_archive

  direct :listing do |kind, options|
    route_for "#{kind}_archive", options
  end

  direct :archive_for do |article, options|
    route_for :yearly_archive, article, options
  end

  direct :topic_page do |topic, options|
    route_for :forum_thread, topic, options
  end

  direct :article_slug do |article, options|
    route_for :slugged_article, article, article.slug, options
  end

  direct :reply do |comment, options|
    route_for "article_#{comment.kind}", comment.article, comment, options
  end
end
"##)
        .write("app/models/article.rb", r##"class Article < ApplicationRecord
  has_many :comments

  def permalink
    Rails.application.routes.url_helpers.article_slug_path(self)
  end

  def permalink_to(anchor)
    Rails.application.routes.url_helpers.article_slug_path(self, anchor: anchor)
  end

  def permalink_for(query)
    Rails.application.routes.url_helpers.article_slug_path(self, q: query)
  end

  def permalink_keyed(query)
    Rails.application.routes.url_helpers.article_slug_path(self, "q" => query)
  end

  def self.archive_link(slug)
    Rails.application.routes.url_helpers.archive_for_path(Article.find_by(slug: slug))
  end

  def self.listing
    Rails.application.routes.url_helpers.listing_path("yearly")
  end
end
"##)
        .write("app/models/comment.rb", r##"class Comment < ApplicationRecord
  belongs_to :article

  def kind
    "comment"
  end

  def link
    Rails.application.routes.url_helpers.reply_path(self)
  end
end
"##)
        .write("app/models/topic.rb", r##"class Topic < ApplicationRecord
  def to_param
    "#{id}-#{name}"
  end

  def link
    Rails.application.routes.url_helpers.topic_page_path(self)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "topics", force: :cascade do |t|
    t.string "name"
  end
  create_table "articles", force: :cascade do |t|
    t.string "title"
    t.string "slug"
  end
  create_table "comments", force: :cascade do |t|
    t.integer "article_id"
  end
end
"##)
}

// Rails: "/read/1/hello-world", "/read/1/hello-world#top",
// "/read/1/hello-world?q=x+y", "/read/1/hello-world" (a nil query value
// is left out), "/articles/1/comments/1", "/topics/1-ruby" (the model's
// own `to_param`), "/read/1/hello-world?q=x+y" (a String key),
// "/archive" (a nil record leaves the optional segment out),
// "/archive/1", "/archive" (an interpolated name, no segment).
const SCRIPT: &str = r##"article = Article.create!(title: "Hello World", slug: "hello-world")
comment = Comment.create!(article: article)
path = "/read/#{article.id}/hello-world"
raise "permalink: #{article.permalink}" unless article.permalink == path
raise "anchor: #{article.permalink_to("top")}" unless article.permalink_to("top") == path + "#top"
raise "query: #{article.permalink_for("x y")}" unless article.permalink_for("x y") == path + "?q=x+y"
raise "nil query: #{article.permalink_for(nil)}" unless article.permalink_for(nil) == path
raise "reply: #{comment.link}" unless comment.link == "/articles/#{article.id}/comments/#{comment.id}"
topic = Topic.create!(name: "ruby")
raise "to_param: #{topic.link}" unless topic.link == "/topics/#{topic.id}-ruby"
raise "string key: #{article.permalink_keyed("x y")}" unless article.permalink_keyed("x y") == path + "?q=x+y"
raise "nil record: #{Article.archive_link("none")}" unless Article.archive_link("none") == "/archive"
raise "record: #{Article.archive_link("hello-world")}" unless Article.archive_link("hello-world") == "/archive/#{article.id}"
raise "listing: #{Article.listing}" unless Article.listing == "/archive"
puts "probe passed"
"##;

#[test]
fn a_direct_route_helper_called_from_a_model_answers_as_rails_does() {
    let run = models().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_direct_route_helper_called_from_a_model_answers_as_rails_does_on_spinel() {
    let run = models().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

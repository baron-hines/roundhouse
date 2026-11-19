//! A `direct` route helper is a function of what its caller passes:
//! `article_slug_path(article)` hands the block the RECORD, and the
//! block's `route_for :slugged_article, article, article.slug, options`
//! fills the target's segments with each argument's `to_param` and
//! renders the options the caller gave (a query key, `anchor:`). A
//! `route_for` whose name is interpolated (`"article_#{…}"`) calls the
//! helper of that name.
//!
//! The helpers are called from a test, a controller and a view. Run
//! first in the generated blog on Rails 8.1 (`bin/rails test`). The
//! Spinel half is `direct_route_model_calls`, which calls the same
//! helpers from model methods.

use super::emit_and_run;

const FILE: &str = "test/controllers/direct_routes_test.rb";
/// Where the emit puts it: a test class whose name names no controller
/// lands beside the model tests.
const EMITTED: &str = "test/models/direct_routes_test.rb";

const ROUTES: &str = r##"Rails.application.routes.draw do
  root "articles#index"

  resources :articles do
    resources :comments, only: [:create, :destroy]
  end

  get "/read/:id/:slug", to: "articles#show", constraints: { id: /\d+/ }, as: :slugged_article

  direct :article_slug do |article, options|
    route_for :slugged_article, article, article.slug, options
  end

  direct :reply do |comment, options|
    route_for "article_#{comment.kind}", comment.article, comment, options
  end
end
"##;

const TEST: &str = r##"require "test_helper"

class DirectRoutesTest < ActionDispatch::IntegrationTest
  test "a direct helper is given the record" do
    article = articles(:one)
    assert_equal "/read/#{article.id}/getting-started-with-rails", article_slug_path(article)
    assert_equal "/read/#{article.id}/getting-started-with-rails#top", article_slug_path(article, anchor: "top")
    assert_equal "/read/#{article.id}/getting-started-with-rails?q=rails", article_slug_path(article, q: "rails")
  end

  test "an interpolated route_for name calls that helper" do
    comment = comments(:two)
    assert_equal "/articles/#{comment.article_id}/comments/#{comment.id}", reply_path(comment)
  end

  test "a controller redirects through a direct helper" do
    article = articles(:two)
    patch article_url(article), params: { article: { title: "Fresh Title" } }
    assert_redirected_to "http://www.example.com/read/#{article.id}/fresh-title"
  end

  test "a view links through a direct helper" do
    article = articles(:one)
    get article_slug_path(article)
    assert_response :success
    assert_select "a[href=?]", "/read/#{article.id}/getting-started-with-rails#comments"
  end
end
"##;

fn blog() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("config/routes.rb", ROUTES)
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true\n\n  def slug\n    title.parameterize\n  end\n",
        )
        .edit(
            "app/models/comment.rb",
            "  validates :body, presence: true\n",
            "  validates :body, presence: true\n\n  def kind\n    \"comment\"\n  end\n",
        )
        .edit(
            "app/controllers/articles_controller.rb",
            "format.html { redirect_to @article, notice: \"Article was successfully updated.\", status: :see_other }",
            "format.html { redirect_to article_slug_url(@article), notice: \"Article was successfully updated.\", status: :see_other }",
        )
        .edit(
            "app/views/articles/show.html.erb",
            "  <h1 class=\"font-bold text-4xl\"><%= @article.title %></h1>\n",
            "  <h1 class=\"font-bold text-4xl\"><%= @article.title %></h1>\n  <%= link_to \"Permalink\", article_slug_path(@article, anchor: \"comments\") %>\n",
        )
        .write(FILE, TEST)
}

#[test]
fn a_direct_route_helper_is_given_the_record() {
    let run = blog().run_test(EMITTED);
    run.assert_passes();
}

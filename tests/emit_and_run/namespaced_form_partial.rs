//! A partial handed the form builder by a namespaced form and a plain one (lobsters' `stories/_form`, rendered from
//! `form_with model: [:mod, @story]` and from `form_with model: @story`): `[:mod, @story]` names the record `@story`,
//! so both renders bind the partial alike and its `f.label` / `f.text_field` / `f.object` render the record's fields.
//! They used to bind it differently, which left it unbound: the partial called `f`, a local nothing defined.

use super::emit_and_run;

const PARTIAL: &str = "<%= f.label :title %>\n<%= f.text_field :title %>\n<p>record: <%= f.object.title %></p>\n";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/drafts/new\", to: \"drafts#new\"\n  namespace :admin do\n    resources :articles, only: [:edit, :update]\n  end\n",
        )
        .write("app/controllers/drafts_controller.rb", "class DraftsController < ApplicationController\n  def new\n    @article = Article.new(title: \"Fresh\")\n  end\nend\n")
        .write(
            "app/controllers/admin/articles_controller.rb",
            "class Admin::ArticlesController < ApplicationController\n  def edit\n    @article = Article.find(params[:id])\n  end\n\n  def update\n    head :ok\n  end\nend\n",
        )
        .write("app/views/articles/_fields.html.erb", PARTIAL)
        .write(
            "app/views/drafts/new.html.erb",
            "<%= form_with model: @article do |f| %>\n  <%= render partial: \"articles/fields\", locals: { article: @article, f: f } %>\n<% end %>\n",
        )
        .write(
            "app/views/admin/articles/edit.html.erb",
            "<%= form_with model: [:admin, @article] do |f| %>\n  <%= render partial: \"articles/fields\", locals: { article: @article, f: f } %>\n<% end %>\n",
        )
}

const TEST: &str = r#"require "test_helper"

class DraftsControllerTest < ActionDispatch::IntegrationTest
  test "the plain form's partial renders the record's fields" do
    get "/drafts/new"
    assert_response :success
    assert_includes response.body, %(<label for="article_title">Title</label>)
    assert_includes response.body, %(name="article[title]" id="article_title")
    assert_includes response.body, %(value="Fresh")
    assert_includes response.body, "record: Fresh"
  end

  test "the namespaced form's partial renders the same fields" do
    article = Article.first
    get "/admin/articles/#{article.id}/edit"
    assert_response :success
    assert_includes response.body, %(action="/admin/articles/#{article.id}")
    assert_includes response.body, %(name="article[title]" id="article_title")
    assert_includes response.body, "record: #{article.title}"
  end
end
"#;

#[test]
fn a_partial_rendered_from_a_namespaced_form_and_a_plain_one_renders_both() {
    app()
        .write("test/controllers/drafts_controller_test.rb", TEST)
        .run_test("test/controllers/drafts_controller_test.rb")
        .assert_passes();
}

/// Renders that hand the partial builders of different records (`article:` and `comment:`) still cannot share one
/// binding: that is an error at the partial, not a partial calling an undefined `f`.
#[test]
fn a_partial_bound_differently_by_its_renders_is_an_error() {
    let (_tree, errors) = app()
        .write(
            "app/views/drafts/new.html.erb",
            "<%= form_with model: Comment.new do |f| %>\n  <%= render partial: \"articles/fields\", locals: { comment: Comment.new, f: f } %>\n<% end %>\n",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("partial `articles/_fields` is handed form builders that bind it differently")), "{errors:#?}");
}

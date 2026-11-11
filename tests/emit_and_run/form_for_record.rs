//! `form_for(record, …)` renders as Rails' does: form_with's model form, the `<form>` given the class and id
//! `apply_form_for_options!` derives from the record's state (`new_article`, `edit_article`), `as:` naming the
//! fields (Devise's `form_for(resource, as: resource_name, url: session_path(resource_name))`).

use super::emit_and_run;

const CONTROLLER: &str = "class DraftsController < ApplicationController\n  def new\n    @article = Article.new\n  end\n\n  def edit\n    @article = Article.first\n  end\nend\n";

const NEW_VIEW: &str = "<%= form_for(@article, url: articles_path) do |f| %>\n  <%= f.text_field :title %>\n<% end %>\n";

const EDIT_VIEW: &str = "<%= form_for(@article, as: :draft, url: article_path(@article), html: { class: \"drafty\" }) do |f| %>\n  <%= f.text_field :title %>\n<% end %>\n";

const TEST: &str = r#"require "test_helper"

class DraftsControllerTest < ActionDispatch::IntegrationTest
  test "a new record's form" do
    get "/drafts/new"
    assert_response :success
    assert_includes response.body, %(<form class="new_article" id="new_article" action="/articles" accept-charset="UTF-8" method="post">)
    assert_includes response.body, %(name="article[title]")
    assert_not_includes response.body, %(name="_method")
  end

  test "a persisted record's form, named and classed" do
    get "/drafts/edit"
    assert_response :success
    article = Article.first
    assert_includes response.body, %(<form class="drafty" id="edit_draft" action="/articles/#{article.id}" accept-charset="UTF-8" method="post">)
    assert_includes response.body, %(<input type="hidden" name="_method" value="patch")
    assert_includes response.body, %(name="draft[title]" id="draft_title")
  end
end
"#;

#[test]
fn form_for_renders_as_rails_does() {
    emit_and_run::real_blog()
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  get \"/drafts/new\", to: \"drafts#new\"\n  get \"/drafts/edit\", to: \"drafts#edit\"\n")
        .write("app/controllers/drafts_controller.rb", CONTROLLER)
        .write("app/views/drafts/new.html.erb", NEW_VIEW)
        .write("app/views/drafts/edit.html.erb", EDIT_VIEW)
        .write("test/controllers/drafts_controller_test.rb", TEST)
        .run_test("test/controllers/drafts_controller_test.rb")
        .assert_passes();
}

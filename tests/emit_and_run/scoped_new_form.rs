//! A form for a record built off a scope (`@bot = User.active_bots.new` in
//! campfire's Accounts::BotsController) names its fields after the record's
//! model, as Rails' param_key does, not after the view's directory or the
//! local the builder's partial receives it as.

#[test]
fn a_form_for_a_record_built_off_a_scope_names_fields_after_its_model() {
    super::emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  include Article::Titled\n\n  validates :title, presence: true\n",
        )
        .write(
            "app/models/article/titled.rb",
            "module Article::Titled\n  extend ActiveSupport::Concern\n\n  included do\n    scope :titled, -> { where.not(title: nil) }\n  end\nend\n",
        )
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  resources :drafts, only: :new\n")
        .write(
            "app/controllers/drafts_controller.rb",
            "class DraftsController < ApplicationController\n  def new\n    @draft = Article.titled.new\n  end\nend\n",
        )
        .write(
            "app/views/drafts/new.html.erb",
            "<%= form_with model: @draft, url: articles_path do |form| %>\n  <%= render \"drafts/form\", form: form, draft: @draft %>\n<% end %>\n",
        )
        .write("app/views/drafts/_form.html.erb", "<%= form.text_field :title %>\n")
        .run_ruby(
            r##"
require "stringio"
env = { "REQUEST_METHOD" => "GET", "PATH_INFO" => "/drafts/new", "QUERY_STRING" => "", "HTTP_ACCEPT" => "text/html" }
status, html = Main.dispatch_core(env, StringIO.new(""))
raise "GET /drafts/new answered #{status}" unless status == 200
field = html[/<input[^>]*type="text"[^>]*>/]
raise "expected article[title] as Rails names it, got #{field.inspect}" unless field&.include?('name="article[title]"') && field.include?('id="article_title"')
puts "scoped new form names fields after its model"
"##,
        )
        .assert_passes();
}

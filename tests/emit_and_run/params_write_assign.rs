//! lobsters' `StoriesController#story_params` writes into its permitted
//! params (`ps[:tags] = Tag.where(tag: ps[:tags] || …)`) and
//! `update_story_attributes` hands the result to `@story.attributes =`.
//! The permit lowers to a synthesized record with no `[]=`, and no
//! runtime defines `attributes=`, so POST /stories raised on the ruby
//! family. Now the write sets the field and its presence flag (the
//! field's type widened to every type written into it), `attributes=` /
//! `assign_attributes` reach the model's typed `assign_from_<class>`,
//! and a permitted `has_many :through` key is assigned through its
//! writer once it holds records.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  post \"/retitle/:id\", to: \"retitles#update\"\n  post \"/rename/:id\", to: \"retitles#rename\"\n",
        )
        .edit(
            "db/schema.rb",
            "  create_table \"comments\", force: :cascade do |t|",
            "  create_table \"labels\", force: :cascade do |t|\n    t.string \"name\"\n  end\n\n  create_table \"labelings\", force: :cascade do |t|\n    t.integer \"article_id\", null: false\n    t.integer \"label_id\", null: false\n  end\n\n  create_table \"comments\", force: :cascade do |t|",
        )
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy\n",
            "  has_many :comments, dependent: :destroy\n  has_many :labelings\n  has_many :labels, through: :labelings\n",
        )
        .write("app/models/label.rb", "class Label < ApplicationRecord\nend\n")
        .write(
            "app/models/labeling.rb",
            "class Labeling < ApplicationRecord\n  belongs_to :article\n  belongs_to :label\nend\n",
        )
        .write(
            "app/controllers/retitles_controller.rb",
            r#"class RetitlesController < ApplicationController
  def update
    @article = Article.find(params[:id])
    ps = params.require(:article).permit(:title, :body, labels: [])
    ps[:title] = ps[:title].to_s.upcase
    ps[:labels] = Label.where(name: ps[:labels] || [])
    @article.attributes = ps
    @article.save!
    head :ok
  end

  def rename
    @article = Article.find(params[:id])
    @article.assign_attributes(params[:keep_body] ? article_params : article_params.except(:body))
    @article.save!
    head :ok
  end

  private

  def article_params
    ps = params.require(:article).permit(:title, :body)
    ps[:title] = ps[:title].to_s.strip
    ps
  end
end
"#,
        )
}

const ASSERTIONS: &str = r#"
require "stringio"
ActionController::Base.allow_forgery_protection = false
def post_form(path, body, query = "")
  env = {
    "REQUEST_METHOD" => "POST", "PATH_INFO" => path, "QUERY_STRING" => query,
    "CONTENT_TYPE" => "application/x-www-form-urlencoded",
    "CONTENT_LENGTH" => body.bytesize.to_s, "HTTP_ACCEPT" => "text/html"
  }
  status, _body = Main.dispatch_core(env, StringIO.new(body))
  status
end
article = Article.create!(title: "Seeded", body: "A body long enough for validation.")
%w[ruby rails other].each { |name| Label.create!(name: name) }
status = post_form("/retitle/#{article.id}", "article%5Btitle%5D=hello&article%5Blabels%5D%5B%5D=ruby&article%5Blabels%5D%5B%5D=rails")
raise "retitle status #{status}" unless status == 200
saved = Article.find(article.id)
raise "the written title #{saved.title.inspect}" unless saved.title == "HELLO"
raise "an absent key overwrote the body: #{saved.body.inspect}" unless saved.body == "A body long enough for validation."
labels = saved.labels.map(&:name).sort
raise "labels through the writer #{labels.inspect}" unless labels == %w[rails ruby]
status = post_form("/rename/#{article.id}", "article%5Btitle%5D=++spaced++&article%5Bbody%5D=Another+body+long+enough")
raise "rename status #{status}" unless status == 200
saved = Article.find(article.id)
raise "except(:body) branch #{saved.title.inspect} #{saved.body.inspect}" unless saved.title == "spaced" && saved.body == "A body long enough for validation."
status = post_form("/rename/#{article.id}", "article%5Btitle%5D=kept&article%5Bbody%5D=Another+body+long+enough", "keep_body=1")
raise "rename keep_body status #{status}" unless status == 200
saved = Article.find(article.id)
raise "whole-record branch #{saved.title.inspect} #{saved.body.inspect}" unless saved.title == "kept" && saved.body == "Another body long enough"
raise "labels survive an assign without the key #{saved.labels.map(&:name).inspect}" unless saved.labels.map(&:name).sort == %w[rails ruby]
puts "params write and assign passed"
"#;

#[test]
fn a_params_write_and_attributes_assignment_save_what_rails_saves() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("params write and assign passed"), "{}", run.stdout);
}

/// The widened field (`Array[String] | Relation`) and the class test in
/// `assign_from_*` compile natively.
#[test]
#[ignore = "requires the Spinel toolchain"]
fn the_widened_field_assigns_through_the_writer_on_spinel() {
    let run = app().run_spinel_with_db(
        r#"
article = Article.create!(title: "Seeded", body: "A body long enough for validation.")
["ruby", "rails", "other"].each { |name| Label.create!(name: name) }
ps = RetitlesArticleParams.from_raw({"article" => {"title" => "hello", "labels" => ["ruby", "rails"]}})
ps.labels = Label.where(name: ["ruby", "rails"])
article.assign_from_retitles_article_params(ps)
article.save!
saved = Article.find(article.id)
puts saved.labels.map { |l| l.name }.sort.inspect
puts saved.title
"#,
    );
    run.assert_passes();
    assert!(run.stdout.contains("[\"rails\", \"ruby\"]\nhello"), "{}", run.stdout);
}

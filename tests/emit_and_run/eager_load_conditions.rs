//! A condition on an `includes`d association (mission_control-jobs' `executions.includes(:job)` then
//! `executions.where(job: { queue_name: … })`, each in its own method): Rails reads the association's name in the
//! condition and loads it with a LEFT OUTER JOIN aliased to that name, or to the table when the condition names the
//! table. The query used to keep the preload alone, and SQLite rejected `"article"."title"`. `EXPECTED` is what Rails
//! 8.1.4 answers for the same data (`conditions_on_an_included_association_match_rails`).

#[path = "../support/prerequisite.rs"]
mod prerequisite;

use super::emit_and_run;

const FEED: &str = r#"class Feed
  def by_association
    Comment.order(:id).includes(:article).where(article: { title: "First" }).map(&:commenter)
  end

  def by_table
    Comment.order(:id).includes(:article).where(articles: { title: "Second" }).map(&:commenter)
  end

  def split
    Comment.order(:id)
      .then { |comments| preload(comments) }
      .then { |comments| titled(comments) }
      .map(&:commenter)
  end

  private

  def preload(comments)
    comments.includes(:article)
  end

  def titled(comments)
    comments.where(article: { title: "First" })
  end
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  get \"/feed\", to: \"feeds#index\"\n")
        .write("app/models/feed.rb", FEED)
        .write(
            "app/controllers/feeds_controller.rb",
            "class FeedsController < ApplicationController\n  def index\n    feed = Feed.new\n    render plain: \"#{feed.by_association.join(\",\")} #{feed.by_table.join(\",\")} #{feed.split.join(\",\")}\"\n  end\nend\n",
        )
}

const SEED: &str = r#"first = Article.create!(title: "First", body: "An article body long enough")
second = Article.create!(title: "Second", body: "Another body long enough")
Comment.create!(article: first, commenter: "Ann", body: "A comment")
Comment.create!(article: second, commenter: "Bob", body: "A comment")
Comment.create!(article: first, commenter: "Cy", body: "A comment")
"#;

const EXPECTED: &str = "Ann,Cy Bob Ann,Cy";

#[test]
fn conditions_on_an_included_association_join_it() {
    app()
        .run_ruby(&format!(
            r##"{SEED}status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/feed", "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
raise "GET /feed answered #{{status}}" unless status == 200
text = +""
body.each {{ |part| text << part }}
raise "feed: #{{text.inspect}}" unless text == {EXPECTED:?}
puts "eager load conditions passed"
"##
        ))
        .assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn conditions_on_an_included_association_join_it_on_spinel() {
    app()
        .run_spinel_with_db(&format!(
            r##"{SEED}controller = FeedsController.new
controller.request_method = "GET"
controller.params = {{}}
controller.process_action(:index)
raise "feed: #{{controller.body}}" unless controller.body == {EXPECTED:?}
puts "eager load conditions passed"
"##
        ))
        .assert_passes();
}

#[test]
fn conditions_on_an_included_association_match_rails() {
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/rails_oracle.rb");
    let script = format!(
        r##"require {helper:?}
unless RailsOracle.available?("activerecord")
  warn "skipping eager load oracle: #{{RailsOracle.label}} needs activerecord"
  exit 77
end
RailsOracle.activate("activerecord")
require "active_record"
ActiveRecord::Base.logger = nil
ActiveRecord::Migration.verbose = false
ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
ActiveRecord::Schema.define do
  create_table(:articles) {{ |t| t.string :title; t.text :body }}
  create_table(:comments) {{ |t| t.integer :article_id; t.string :commenter; t.text :body }}
end
class Article < ActiveRecord::Base
  has_many :comments
end
class Comment < ActiveRecord::Base
  belongs_to :article
end
{FEED}{SEED}feed = Feed.new
print "#{{feed.by_association.join(",")}} #{{feed.by_table.join(",")}} #{{feed.split.join(",")}}"
"##
    );
    let output = std::process::Command::new("ruby").args(["-e", &script]).output().expect("Rails oracle");
    if output.status.code() == Some(77) {
        prerequisite::missing(String::from_utf8_lossy(&output.stderr).trim());
        return;
    }
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED);
}

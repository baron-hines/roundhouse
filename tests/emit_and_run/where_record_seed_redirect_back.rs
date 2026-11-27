//! Two lobsters POSTs (`/stories/:id/hide`, `/save`) that the HTTP replay
//! (scripts/lobsters-http-compare) caught failing on the ruby family:
//!
//! * `HiddenStory.where(story: story, user: user).first_or_initialize`
//!   lowers its keys to `story_id` / `user_id` with the RECORDS as values.
//!   The query reads each record's id, but the new record was seeded with
//!   the record itself, so `save!` raised "Story must exist" (422).
//! * `redirect_back_or_to` without a Referer header: the CRuby overlay's
//!   `request.referer` is nil, and `nil.start_with?` was a 500. Rails
//!   goes to the fallback.

use super::emit_and_run;

const ASSERTIONS: &str = r#"
article = Article.create!(title: "Seeded", body: "A body long enough for validation.")
status, headers, _body = Main.run_rack(
  "REQUEST_METHOD" => "GET",
  "PATH_INFO" => "/hold/#{article.id}",
  "QUERY_STRING" => "",
  "HTTP_HOST" => "localhost",
  "rack.input" => StringIO.new("")
)
raise "status #{status}" unless status == 302
raise "location #{headers.inspect}" unless headers["location"].to_s.end_with?("/fallback")
held = Comment.where(article_id: article.id).to_a
raise "held #{held.map(&:article_id).inspect}" unless held.size == 1 && held[0].article_id == article.id
puts "seeded hold passed"
"#;

#[test]
fn a_record_keyed_where_seeds_its_id_and_redirect_back_falls_back() {
    let run = emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/hold/:id\", to: \"holds#create\"\n",
        )
        .write(
            "app/controllers/holds_controller.rb",
            r#"class HoldsController < ApplicationController
  def create
    article = Article.find(params[:id])
    comment = Comment.where(article: article).first_or_initialize
    comment.commenter = "holder"
    comment.body = "held"
    comment.save!
    redirect_back_or_to "/fallback"
  end
end
"#,
        )
        .run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("seeded hold passed"), "{}", run.stdout);
}

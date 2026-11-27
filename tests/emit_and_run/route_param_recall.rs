//! A route helper called without the segments its route requires recalls
//! them from the current request, as Rails' `url_for` does. lobsters'
//! `/~bob` builds its moderator forms with a bare `user_ban_path`, and
//! `/~bob/comments.rss` its feed link with `user_comments_url(format:
//! :rss)`; both were `wrong number of arguments (given 0, expected 1)`.
//!
//! Two neighbours of the same lobsters pages ride along: `Relation#model`
//! (Rails' alias for `klass`, read by the moderator dashboards) and a bare
//! `mail` in an after_action, which answers the mail the action built
//! (the blocklist check every lobsters mailer runs).

use super::emit_and_run;

const TEST_PATH: &str = "test/controllers/peeks_controller_test.rb";

fn overlay() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "    resources :comments, only: [:create, :destroy]\n  end\n",
            concat!(
                "    resources :comments, only: [:create, :destroy]\n  end\n",
                "  get \"/peeks/:name\" => \"peeks#show\", as: \"peek\"\n",
                "  get \"/peeks/:name/share\" => \"peeks#share\", as: \"share_peek\"\n",
                "  get \"/peeks/:name/feed\" => \"peeks#feed\", as: \"peek_feed\"\n",
            ),
        )
        .write(
            "app/controllers/peeks_controller.rb",
            concat!(
                "class PeeksController < ApplicationController\n",
                "  def show\n    @shown = params[:name]\n  end\n\n",
                "  def share\n    redirect_to peek_path\n  end\n\n",
                "  def feed\n    render plain: peek_feed_url(format: :rss)\n  end\n",
                "end\n",
            ),
        )
        .write(
            "app/views/peeks/show.html.erb",
            "<p id=\"name\"><%= @shown %></p>\n<a id=\"share\" href=\"<%= share_peek_path %>\">share</a>\n",
        )
        .write(
            TEST_PATH,
            r#"require "test_helper"

class PeeksControllerTest < ActionDispatch::IntegrationTest
  test "a view recalls the segment from the request" do
    get "/peeks/ruby"
    assert_response :success
    assert_select "a#share[href=?]", "/peeks/ruby/share"
  end

  test "a controller recalls it too, beside the options it names" do
    get "/peeks/ruby/share"
    assert_redirected_to "/peeks/ruby"
    get "/peeks/ruby/feed"
    assert_equal "http://www.example.com/peeks/ruby/feed.rss", response.body
  end
end
"#,
        )
}

#[test]
fn a_missing_segment_is_recalled_from_the_request() {
    overlay().run_test(TEST_PATH).assert_passes();
}

#[test]
fn a_relation_answers_its_model() {
    let run = emit_and_run::real_blog().run_ruby(
        "raise \"model\" unless Article.where(title: \"x\").model == Article\nraise \"table\" unless Article.all.model.table_name == \"articles\"\nputs \"model passed\"\n",
    );
    run.assert_passes();
    assert!(run.stdout.contains("model passed"), "{}", run.stdout);
}

#[test]
fn a_bare_mail_in_an_after_action_reads_the_built_mail() {
    let run = emit_and_run::real_blog()
        .write(
            "app/mailers/guard_mailer.rb",
            concat!(
                "class GuardMailer < ApplicationMailer\n",
                "  after_action :hold\n\n",
                "  def note(to)\n    mail(to: to, subject: \"Held\")\n  end\n\n",
                "  private\n\n",
                "  def hold\n    mail.perform_deliveries = false if mail.to[0] == \"blocked@example.com\"\n  end\n",
                "end\n",
            ),
        )
        .write("app/views/guard_mailer/note.text.erb", "A note.\n")
        .run_ruby(concat!(
            "GuardMailer.note(\"blocked@example.com\").deliver_now\n",
            "raise \"delivered: #{ActionMailer::Base.deliveries.size}\" unless ActionMailer::Base.deliveries.empty?\n",
            "GuardMailer.note(\"reader@example.com\").deliver_now\n",
            "raise \"held: #{ActionMailer::Base.deliveries.size}\" unless ActionMailer::Base.deliveries.size == 1\n",
            "puts \"after_action mail passed\"\n",
        ));
    run.assert_passes();
    assert!(run.stdout.contains("after_action mail passed"), "{}", run.stdout);
}

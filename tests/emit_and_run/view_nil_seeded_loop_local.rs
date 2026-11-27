//! A view local seeded with nil and written inside a loop is read as
//! nil-or-the-written-type everywhere, including earlier in the same loop
//! body. lobsters' comments/_threads tracks `previous_depth = nil` and
//! `top_comment_depth ||= comment.depth` across `thread.each`. A view is
//! typed once, so its reads saw the bare nil seed: `Integer > nil` and
//! `no known method - on nil` were reported as errors although the page
//! renders.

use super::emit_and_run;

const TEST_PATH: &str = "test/controllers/loop_locals_controller_test.rb";

#[test]
fn a_nil_seeded_loop_local_reads_its_written_type() {
    let run = emit_and_run::real_blog()
        .edit(
            "app/views/articles/show.html.erb",
            "<%= link_to \"Back to articles\"",
            concat!(
                "<% prev = nil %>\n<% top = nil %>\n",
                "<% @article.comments.each do |c| %>\n",
                "  <% top ||= c.id %>\n",
                "  <% if prev %><span class=\"step\"><%= c.id > prev ? c.id - prev : 0 %></span><% end %>\n",
                "  <% prev = c.id %>\n",
                "<% end %>\n",
                "<% if top %><span id=\"span\"><%= prev - top %></span><% end %>\n",
                "<%= link_to \"Back to articles\"",
            ),
        )
        .write(
            TEST_PATH,
            r#"require "test_helper"

class LoopLocalsControllerTest < ActionDispatch::IntegrationTest
  test "the loop local carries the previous iteration's value" do
    article = Article.create!(title: "Loops", body: "A body long enough for validation.")
    first = article.comments.create!(commenter: "a", body: "one")
    article.comments.create!(commenter: "b", body: "two")
    last = article.comments.create!(commenter: "c", body: "three")
    get "/articles/#{article.id}"
    assert_response :success
    assert_select "span.step", 2
    assert_select "span#span", (last.id - first.id).to_s
  end
end
"#,
        )
        .run_test(TEST_PATH);
    run.assert_passes();
    assert!(run.errors.is_empty(), "{:?}", run.errors);
}

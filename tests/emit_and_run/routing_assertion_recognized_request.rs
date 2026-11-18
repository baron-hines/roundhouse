//! Rails' routing assertions answer `recognized_request_for` to a test
//! that overrides `assert_recognizes` and calls `super`, and count their
//! assertions as ActionDispatch's RoutingAssertions do: one per
//! `assert_recognizes`, two per `assert_generates`, so three per
//! `assert_routing`. Ignored as a fork spec until the compiler does; the
//! test passes unchanged in the generated blog on Rails 8.1.4
//! (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/articles_routing_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write(
        FILE,
        r##"require "test_helper"

class ArticlesRoutingControllerTest < ActionDispatch::IntegrationTest
  def assert_recognizes(expected_options, path, extras = {}, msg = nil)
    request = recognized_request_for(path, extras, msg)
    recognized << "#{request.path_parameters[:controller]}##{request.path_parameters[:action]}"
    super
  end

  def recognized
    @recognized ||= []
  end

  test "an override reads the recognized request before super" do
    assert_routing "/articles/7", controller: "articles", action: "show", id: "7"
    assert_equal 3, assertions
    assert_equal ["articles#show"], recognized
    assert_recognizes({ controller: "articles", action: "index" }, "/articles")
    assert_equal 6, assertions
    assert_equal ["articles#show", "articles#index"], recognized
  end
end
"##,
    )
}

#[test]
#[ignore = "fork spec: routes"]
fn routing_assertions_answer_recognized_request_for_and_count() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: routes"]
fn routing_assertions_answer_recognized_request_for_and_count_on_spinel() {
    app().run_spin_test("test/articles_routing_controller_test.rb").assert_passes();
}

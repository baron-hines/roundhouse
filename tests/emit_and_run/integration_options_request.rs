//! An integration test sends an OPTIONS request with `options`, or with
//! `process(:options, ...)`, through its session, as Rails' integration
//! Session answers it; the request reaches the route drawn `via: :options`
//! and the test reads its status and headers. Ignored as a fork spec until
//! the compiler does; the test passes unchanged in the generated blog on
//! Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/preflights_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  match \"/lanterns\", to: \"preflights#show\", via: :options\n",
        )
        .write(
            "app/controllers/preflights_controller.rb",
            r#"class PreflightsController < ApplicationController
  def show
    response.headers["Allow"] = "GET, POST, OPTIONS"
    response.headers["X-Lantern-Origin"] = request.headers["Origin"].to_s
    head :no_content
  end
end
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class PreflightsControllerTest < ActionDispatch::IntegrationTest
  test "options sends an OPTIONS request" do
    options "/lanterns", headers: { "Origin" => "https://kiln.example" }
    assert_response :no_content
    assert_equal "GET, POST, OPTIONS", response.headers["Allow"]
    assert_equal "https://kiln.example", response.headers["X-Lantern-Origin"]
    assert_equal "OPTIONS", request.request_method
  end

  test "process(:options) sends one too" do
    status = process(:options, "/lanterns", headers: { "Origin" => "https://wick.example" })
    assert_equal 204, status
    assert_equal "https://wick.example", response.headers["X-Lantern-Origin"]
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: controller tests"]
fn an_integration_session_sends_an_options_request() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: controller tests"]
fn an_integration_session_sends_an_options_request_on_spinel() {
    app().run_spin_test("test/preflights_controller_test.rb").assert_passes();
}

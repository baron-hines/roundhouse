//! A functional test's controller holds the test's request from setup on,
//! as Rails' ActionController::TestCase builds it in
//! `setup_controller_request_and_response`: before the first `get`,
//! `@controller.request` is `@request`, and a header written through it
//! reaches the action. Ignored as a fork spec until the compiler does; the
//! test passes unchanged in the generated blog on Rails 8.1.4
//! (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/porches_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/porch\", to: \"porches#show\"\n",
        )
        .write(
            "app/controllers/porches_controller.rb",
            r#"class PorchesController < ApplicationController
  def show
    render plain: request.headers["X-Porch-Light"].to_s
  end
end
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class PorchesControllerTest < ActionController::TestCase
  test "the controller holds the test's request before the first get" do
    assert_not_nil @controller.request
    assert_same @request, @controller.request
    @controller.request.headers["X-Porch-Light"] = "on"
    get :show
    assert_response :success
    assert_equal "on", response.body
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: controller tests"]
fn a_functional_tests_controller_holds_its_request_from_setup() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: controller tests"]
fn a_functional_tests_controller_holds_its_request_from_setup_on_spinel() {
    app().run_spin_test("test/porches_controller_test.rb").assert_passes();
}

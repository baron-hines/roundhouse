//! A controller test's request drops every nil inside an Array from its
//! params, as Rails' `perform_deep_munge` does, for a form-encoded request
//! and for one sent `as: :json` alike. A blank String stays, a nil Hash
//! value stays nil, and a JSON request keeps its Integers. Ignored as a
//! fork spec until the compiler does; the tests pass unchanged in the
//! generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/tallies_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  resources :tallies, only: :create\n",
        )
        .write(
            "app/controllers/tallies_controller.rb",
            r#"class TalliesController < ApplicationController
  skip_forgery_protection

  def create
    rows = params[:rows].map { |row| [row[:mark], row[:counts]] }
    render plain: [params[:marks].inspect, rows.inspect].join("|")
  end
end
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class TalliesControllerTest < ActionController::TestCase
  test "a form request drops nil members" do
    post :create, params: { marks: ["x", nil, "", nil], rows: [{ mark: nil, counts: [nil, 2] }] }
    assert_response :success
    assert_equal "[\"x\", \"\"]|[[nil, [\"2\"]]]", response.body
  end

  test "a json request drops nil members" do
    post :create, params: { marks: ["x", nil, "", nil], rows: [{ mark: nil, counts: [nil, 2] }] }, as: :json
    assert_response :success
    assert_equal "[\"x\", \"\"]|[[nil, [2]]]", response.body
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: controller tests"]
fn a_controller_test_drops_nil_array_members_from_its_params() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: controller tests"]
fn a_controller_test_drops_nil_array_members_from_its_params_on_spinel() {
    app().run_spin_test("test/tallies_controller_test.rb").assert_passes();
}

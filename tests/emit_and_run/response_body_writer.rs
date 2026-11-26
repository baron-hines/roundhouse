//! `response.body = ...` replaces what the action rendered, as
//! ActionDispatch::Response#body= does: an around_action rewrites the
//! rendered JSON after `yield`, and the status and content type the action
//! set stay. Ignored as a fork spec until the compiler does; the test
//! passes unchanged in the generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/ledgers_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  resources :ledgers, only: :show\n",
        )
        .write(
            "app/controllers/ledgers_controller.rb",
            r#"class LedgersController < ApplicationController
  around_action :strip_private_fields

  def show
    render json: { "id" => params[:id].to_i, "balance" => 40, "pin" => "1234" }, status: :accepted
  end

  private

  def strip_private_fields
    yield
    payload = JSON.parse(response.body)
    payload.delete("pin")
    response.body = JSON.generate(payload)
  end
end
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class LedgersControllerTest < ActionDispatch::IntegrationTest
  test "body= replaces the rendered body" do
    get ledger_url(3)
    assert_response :accepted
    assert_equal "application/json", response.media_type
    assert_equal({ "id" => 3, "balance" => 40 }, JSON.parse(response.body))
    assert_not_includes response.body, "1234"
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: controller tests"]
fn response_body_writer_replaces_the_rendered_body() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: controller tests"]
fn response_body_writer_replaces_the_rendered_body_on_spinel() {
    app().run_spin_test("test/ledgers_controller_test.rb").assert_passes();
}

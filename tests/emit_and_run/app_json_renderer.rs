//! An app that registers its own `:json` renderer
//! (`ActionController::Renderers.add(:json) { |value, options| ... }` in an
//! initializer) has every `render json:` body made by that block, run on
//! the controller with the render's options, as Rails dispatches it through
//! `_render_with_renderer_json`. Ignored as a fork spec until the compiler
//! does; the test passes unchanged in the generated blog on Rails 8.1.4
//! (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/gauges_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  resources :gauges, only: [:index, :show]\n",
        )
        .write(
            "config/initializers/json_envelope.rb",
            r#"ActionController::Renderers.add(:json) do |value, options|
  self.content_type = Mime[:json] if media_type.nil?
  { "data" => value.as_json, "label" => options[:label], "from" => controller_name }.to_json
end
"#,
        )
        .write(
            "app/controllers/gauges_controller.rb",
            r#"class GaugesController < ApplicationController
  def index
    render json: [1, 2], label: "many"
  end

  def show
    render json: { "reading" => params[:id].to_i }, status: :created
  end
end
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class GaugesControllerTest < ActionDispatch::IntegrationTest
  test "render json runs the app's renderer" do
    get gauges_url
    assert_response :success
    assert_equal "application/json", response.media_type
    assert_equal({ "data" => [1, 2], "label" => "many", "from" => "gauges" }, JSON.parse(response.body))
  end

  test "the renderer keeps the render's status" do
    get gauge_url(9)
    assert_response :created
    assert_equal({ "data" => { "reading" => 9 }, "label" => nil, "from" => "gauges" }, JSON.parse(response.body))
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: controller tests"]
fn render_json_runs_the_apps_own_renderer() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: controller tests"]
fn render_json_runs_the_apps_own_renderer_on_spinel() {
    app().run_spin_test("test/gauges_controller_test.rb").assert_passes();
}

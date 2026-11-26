//! `render json:`'s own options reach the value's `as_json`, as Rails'
//! json renderer encodes `json.to_json(options.except(:callback,
//! :content_type, :status))`: an Array and a Hash hand them on to each
//! member's `as_json`, and `status:` still sets the status. Ignored as a
//! fork spec until the compiler does; the test passes unchanged in the
//! generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/badges_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  resources :badges, only: [:index, :show]\n",
        )
        .write(
            "app/models/badge.rb",
            r#"class Badge
  def initialize(name)
    @name = name
  end

  def as_json(options = nil)
    viewer = options ? options[:viewer] : nil
    { "name" => @name, "viewer" => viewer }
  end
end
"#,
        )
        .write(
            "app/controllers/badges_controller.rb",
            r#"class BadgesController < ApplicationController
  def index
    render json: [Badge.new("gold"), Badge.new("tin")], viewer: "curator"
  end

  def show
    render json: { "badge" => Badge.new("bronze") }, viewer: "guest", status: :accepted
  end
end
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class BadgesControllerTest < ActionDispatch::IntegrationTest
  test "an Array hands the options to each member" do
    get badges_url
    assert_response :success
    assert_equal [{ "name" => "gold", "viewer" => "curator" }, { "name" => "tin", "viewer" => "curator" }], JSON.parse(response.body)
  end

  test "a Hash hands them on too" do
    get badge_url(1)
    assert_response :accepted
    assert_equal({ "badge" => { "name" => "bronze", "viewer" => "guest" } }, JSON.parse(response.body))
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: controller tests"]
fn render_json_options_reach_as_json() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: controller tests"]
fn render_json_options_reach_as_json_on_spinel() {
    app().run_spin_test("test/badges_controller_test.rb").assert_passes();
}

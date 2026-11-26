//! A view's `url_for(Hash)` is the route set's path under the rendering
//! controller, as Rails' RoutingUrlFor answers it: the request's
//! controller and path parameters are recalled, so `{ action: :survey }`
//! on `/hives/5` is `/hives/5/survey`, a given `id:` replaces the
//! recalled one, and a String passes as written. A helper's call answers
//! the same. Ignored as a fork spec until the compiler does; the test
//! passes unchanged in the generated blog on Rails 8.1.4
//! (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/hives_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  resources :hives, only: :show do\n    member do\n      get :survey\n    end\n  end\n",
        )
        .write(
            "app/helpers/hives_helper.rb",
            "module HivesHelper\n  def survey_link(id)\n    url_for({ action: :survey, id: id })\n  end\nend\n",
        )
        .write(
            "app/controllers/hives_controller.rb",
            "class HivesController < ApplicationController\n  def show\n  end\n\n  def survey\n    head :ok\n  end\nend\n",
        )
        .write(
            "app/views/hives/show.html.erb",
            "<p id=\"same\"><%= url_for(action: :survey) %></p>\n<p id=\"other\"><%= url_for(action: :survey, id: 9) %></p>\n<p id=\"helper\"><%= survey_link(12) %></p>\n<p id=\"string\"><%= url_for(\"/meadow\") %></p>\n",
        )
        .write(
            FILE,
            r#"require "test_helper"

class HivesControllerTest < ActionDispatch::IntegrationTest
  test "url_for of a Hash is the route under the rendering controller" do
    get "/hives/5"
    assert_response :success
    assert_includes response.body, "<p id=\"same\">/hives/5/survey</p>"
    assert_includes response.body, "<p id=\"other\">/hives/9/survey</p>"
    assert_includes response.body, "<p id=\"helper\">/hives/12/survey</p>"
    assert_includes response.body, "<p id=\"string\">/meadow</p>"
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: views helpers"]
fn a_views_url_for_hash_is_the_route_under_its_controller() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: views helpers"]
fn a_views_url_for_hash_is_the_route_under_its_controller_on_spinel() {
    app().run_spin_test("test/hives_controller_test.rb").assert_passes();
}

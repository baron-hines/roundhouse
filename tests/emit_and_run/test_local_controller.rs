//! A controller a test file defines runs as a controller on the routes its
//! test draws, as Rails' `process` runs it: the inner class keeps its
//! parent's filters, answers its nested `controller_path`, and its action
//! is reached through `with_routing`. Ignored as a fork spec until the
//! compiler does; the test passes unchanged in the generated blog on
//! Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/greenhouse_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "app/controllers/nursery_controller.rb",
            r#"class NurseryController < ApplicationController
  before_action :water

  private

  def water
    @watered = "watered"
  end
end
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class GreenhouseControllerTest < ActionController::TestCase
  class SeedlingsController < NurseryController
    def sprout
      render plain: [@watered, controller_path, params[:tray]].join("|")
    end
  end

  tests SeedlingsController

  test "a controller the test defines runs on the routes it draws" do
    with_routing do |set|
      set.draw do
        get "/sprout/:tray", to: "greenhouse_controller_test/seedlings#sprout"
      end
      get :sprout, params: { tray: "4" }
      assert_response :success
      assert_equal "watered|greenhouse_controller_test/seedlings|4", response.body
    end
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: controller tests"]
fn a_controller_a_test_defines_runs_on_its_drawn_routes() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: controller tests"]
fn a_controller_a_test_defines_runs_on_its_drawn_routes_on_spinel() {
    app().run_spin_test("test/greenhouse_controller_test.rb").assert_passes();
}

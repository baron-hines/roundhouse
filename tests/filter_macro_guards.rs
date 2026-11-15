//! A concern's class-side filter macro that forwards its keywords (maybe's `guard_feature(**options)` →
//! `before_action :guard_feature, **options`) called with a lambda guard, and a class-side setter that keeps its
//! `*rest` in an ivar (maybe's `permitted_accountable_attributes`). Ingest refused both ("not filter DSL"); the guard
//! now runs on the controller as a hand-written filter's does, and the call stores the list as the class body runs.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const CONCERN: &str = r#"module FeatureGuardable
  extend ActiveSupport::Concern

  class_methods do
    def guard_feature(**options)
      before_action :guard_feature, **options
    end

    def permitted_extras(*attrs)
      @permitted_extras = attrs if attrs.any?
      @permitted_extras ||= [:id]
    end
  end

  private
    def guard_feature
      render plain: "Feature disabled", status: :forbidden
    end
end
"#;

const CONTROLLER: &str = r#"class ProbesController < ApplicationController
  include FeatureGuardable
  guard_feature if: -> { params[:off] == "1" }
  permitted_extras :name, :rate

  def show
    render plain: "on #{self.class.permitted_extras.join(",")}"
  end
end
"#;

const PLAIN: &str = r#"class PlainsController < ApplicationController
  include FeatureGuardable

  def show
    render plain: "plain #{self.class.permitted_extras.join(",")}"
  end
end
"#;

const TEST: &str = r#"require "test_helper"

class ProbesControllerTest < ActionDispatch::IntegrationTest
  test "the guard runs when its lambda is true" do
    get "/probe", params: { off: "1" }
    assert_response :forbidden
    assert_equal "Feature disabled", response.body
  end

  test "the action runs when it is false, with the stored list" do
    get "/probe"
    assert_response :success
    assert_equal "on name,rate", response.body
  end

  test "a class that never calls the setter reads its default" do
    get "/plain"
    assert_equal "plain id", response.body
  end
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/controllers/concerns/feature_guardable.rb", CONCERN)
        .write("app/controllers/probes_controller.rb", CONTROLLER)
        .write("app/controllers/plains_controller.rb", PLAIN)
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  get \"probe\", to: \"probes#show\"\n  get \"plain\", to: \"plains#show\"\n")
        .write("test/controllers/probes_controller_test.rb", TEST)
}

#[test]
fn a_forwarded_guard_and_a_stored_list_run_on_ruby() {
    app().run_test("test/controllers/probes_controller_test.rb").assert_passes();
}

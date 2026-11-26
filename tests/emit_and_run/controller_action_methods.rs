//! A controller class answers `action_methods`, as AbstractController::Base
//! does: the public instance methods of the controller, its app base and
//! the modules they include, as a Set of Strings, less what
//! ActionController::Base answers and less anything private or protected.
//! Ignored as a fork spec until the compiler does; the test passes
//! unchanged in the generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/shelves_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  resources :shelves, only: [:index, :show]\n",
        )
        .write(
            "app/controllers/concerns/stocktaking.rb",
            r#"module Stocktaking
  def count
    head :ok
  end
end
"#,
        )
        .write(
            "app/controllers/storeroom_controller.rb",
            r#"class StoreroomController < ApplicationController
  def ping
    head :ok
  end
end
"#,
        )
        .write(
            "app/controllers/shelves_controller.rb",
            r#"class ShelvesController < StoreroomController
  include Stocktaking

  def index
    render plain: "shelves"
  end

  def show
    render plain: "shelf #{params[:id]}"
  end

  protected

  def tidy
  end

  private

  def dust
  end
end
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class ShelvesControllerTest < ActionDispatch::IntegrationTest
  test "action_methods lists the public actions" do
    names = ShelvesController.action_methods
    assert_kind_of Set, names
    assert_equal %w[count index ping show], names.to_a.sort
    assert_not names.include?("tidy")
    assert_not names.include?("dust")
    assert_equal %w[ping], StoreroomController.action_methods.to_a.sort
    get shelves_url
    assert_equal "shelves", response.body
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: controller tests"]
fn a_controller_answers_its_action_methods() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: controller tests"]
fn a_controller_answers_its_action_methods_on_spinel() {
    app().run_spin_test("test/shelves_controller_test.rb").assert_passes();
}

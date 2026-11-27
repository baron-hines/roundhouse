//! `helper_method(*helpers)` over a local bound to a literal list, as `DeviseController` writes
//! `helpers = %w(resource scope_name …)`: each name is callable from the controller's views, and the view's call
//! answers the controller method's type, so `greeting.upcase` there is a String's.

use super::emit_and_run;

const CONTROLLER: &str = r#"class GreetingsController < ApplicationController
  helpers = %w(greeting audience)
  helper_method(*helpers)

  def show
  end

  private

  def greeting
    "hello"
  end

  def audience
    "world"
  end
end
"#;

const TEST: &str = r#"require "test_helper"

class GreetingsControllerTest < ActionDispatch::IntegrationTest
  test "a helper_method named in a list renders from the view" do
    get "/greeting"
    assert_response :success
    assert_includes response.body, "HELLO WORLD"
  end
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  get \"/greeting\", to: \"greetings#show\"\n")
        .write("app/controllers/greetings_controller.rb", CONTROLLER)
        .write("app/views/greetings/show.html.erb", "<p><%= greeting.upcase %> <%= audience.upcase %></p>\n")
        .write("test/controllers/greetings_controller_test.rb", TEST)
}

#[test]
fn a_helper_method_list_is_typed_in_the_view() {
    let errors = app().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn a_helper_method_list_renders_from_the_view() {
    app().run_test("test/controllers/greetings_controller_test.rb").assert_passes();
}

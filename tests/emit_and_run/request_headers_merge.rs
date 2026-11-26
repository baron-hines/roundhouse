//! `request.headers` writes through to the request's env, as Rails'
//! `ActionDispatch::Http::Headers` does: `[]=` and `merge!` set each
//! header under its env name (`X-Route` is `HTTP_X_ROUTE`), over any
//! value the client sent. The test below passes unchanged in the generated blog on Rails 8.1
//! (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/signals_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/signal\", to: \"signals#show\"\n",
        )
        .write(
            "app/controllers/signals_controller.rb",
            r#"class SignalsController < ApplicationController
  before_action :route_request

  def show
    render plain: [
      request.headers["X-Route"],
      request.env["HTTP_X_ROUTE"],
      request.headers["X-Leg"],
      request.headers["X-Signal-Origin"],
      request.headers["X-Unsent"].inspect
    ].join("|")
  end

  private

  def route_request
    request.headers.merge!("X-Route" => "north", "X-Signal-Origin" => "replaced")
    request.headers["X-Leg"] = "two"
  end
end
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class SignalsControllerTest < ActionDispatch::IntegrationTest
  test "headers written in a filter reach the action" do
    get "/signal", headers: { "X-Signal-Origin" => "client" }
    assert_response :success
    assert_equal "north|north|two|replaced|nil", response.body
  end
end
"#,
        )
}

#[test]
fn request_headers_merge_writes_the_env() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "requires spin and spinel on PATH plus SQLite development libraries"]
fn request_headers_merge_writes_the_env_on_spinel() {
    let (emitted, errors) = app().emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    let file = "test/signals_controller_test.rb";
    assert!(emitted.join(file).exists(), "{file} was not emitted in {}", emitted.display());
    let output = std::process::Command::new("spin").args(["test", file]).current_dir(&*emitted).output().expect("spawn spin");
    assert!(
        output.status.success(),
        "spin test failed in {}\n=== stdout ===\n{}\n=== stderr ===\n{}",
        emitted.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

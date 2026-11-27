//! A scope's segment requirement named by a constant holds the segment, as
//! Rails' scope requirement does: under `scope "(:locale)", locale:
//! LOCALE_FORMAT` with `LOCALE_FORMAT = /en|fr/` in the routes file,
//! `/fr/shelves/7` binds the locale, `/shelves/7` binds none, and neither
//! `/de/shelves/7` nor `/french/shelves/7` is a route (Journey matches the
//! requirement against the whole segment). Ignored as a fork spec until
//! the compiler does; the test passes unchanged in the generated blog on
//! Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/shelves_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "Rails.application.routes.draw do\n  root \"articles#index\"\n",
            r#"LOCALE_FORMAT = /en|fr/

Rails.application.routes.draw do
  root "articles#index"

  scope "(:locale)", locale: LOCALE_FORMAT do
    get "shelves/:id", to: "shelves#show", as: :shelf
  end
"#,
        )
        .write(
            "app/controllers/shelves_controller.rb",
            r##"class ShelvesController < ApplicationController
  def show
    render plain: "#{params[:locale]}:#{params[:id]}"
  end
end
"##,
        )
        .write(
            FILE,
            r##"require "test_helper"

class ShelvesControllerTest < ActionDispatch::IntegrationTest
  test "the requirement holds the whole locale segment" do
    get "/fr/shelves/7"
    assert_equal "fr:7", response.body
    get "/shelves/7"
    assert_equal ":7", response.body
    get "/de/shelves/7"
    assert_response :not_found
    get "/french/shelves/7"
    assert_response :not_found
  end
end
"##,
        )
}

#[test]
#[ignore = "fork spec: routes"]
fn a_scope_requirement_named_by_a_constant_holds_the_segment() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: routes"]
fn a_scope_requirement_named_by_a_constant_holds_the_segment_on_spinel() {
    app().run_spin_test("test/shelves_controller_test.rb").assert_passes();
}

//! A scope's segment requirement named by a constant the app builds as it
//! loads (`Regexp.union` over a list) matches that Regexp against the
//! whole segment as a request arrives, as Journey anchors a requirement:
//! under `scope "(:locale)", locale: LocaleCode::FORMAT`,
//! `/pt-BR/shelves/7` binds the locale, `/shelves/7` binds none, and
//! `/pt/shelves/7` (a prefix of a code) and `/es/shelves/7` are not
//! routes. Ignored as a fork spec until the compiler does; the test passes
//! unchanged in the generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/shelves_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n\n  scope \"(:locale)\", locale: LocaleCode::FORMAT do\n    get \"shelves/:id\", to: \"shelves#show\", as: :shelf\n  end\n",
        )
        .write(
            "app/models/locale_code.rb",
            r##"module LocaleCode
  CODES = %w[en fr pt-BR].freeze
  FORMAT = Regexp.union(CODES.map { |code| Regexp.new(Regexp.escape(code)) })
end
"##,
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
  test "the built requirement holds the whole locale segment" do
    get "/pt-BR/shelves/7"
    assert_equal "pt-BR:7", response.body
    get "/shelves/7"
    assert_equal ":7", response.body
    get "/pt/shelves/7"
    assert_response :not_found
    get "/es/shelves/7"
    assert_response :not_found
  end
end
"##,
        )
}

#[test]
#[ignore = "fork spec: routes"]
fn a_scope_requirement_built_at_load_holds_the_segment() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: routes"]
fn a_scope_requirement_built_at_load_holds_the_segment_on_spinel() {
    app().run_spin_test("test/shelves_controller_test.rb").assert_passes();
}

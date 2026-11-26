//! A route helper binds its positional arguments to the route's segment
//! keys in path order, as Rails' `handle_positional_args` does: under
//! `scope "(:locale)"`, `shelf_path("de", 6)` is `/de/shelves/6`, from an
//! app class that includes the url helpers and from a test body alike,
//! and arguments that are calls run in the order they are written, before
//! Rails binds them. Ignored as a fork spec until the compiler does; the
//! test passes unchanged in the generated blog on Rails 8.1.4
//! (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/shelves_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n\n  scope \"(:locale)\" do\n    get \"shelves/:id\", to: \"shelves#show\", as: :shelf\n  end\n",
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
            "app/models/shelf_links.rb",
            r##"class ShelfLinks
  class << self
    include Rails.application.routes.url_helpers

    def localized(locale, id)
      shelf_path(locale, id)
    end

    def noted
      @noted ||= []
    end

    def note(value)
      noted << value
      value
    end

    def noted_shelf
      shelf_path(note("nl"), note(3))
    end
  end
end
"##,
        )
        .write(
            FILE,
            r##"require "test_helper"

class ShelvesControllerTest < ActionDispatch::IntegrationTest
  test "positional arguments fill the segments in path order" do
    assert_equal "/de/shelves/6", ShelfLinks.localized("de", 6)
    assert_equal "/it/shelves/9", shelf_path("it", 9)
    get shelf_path("fr", 4)
    assert_equal "fr:4", response.body
    assert_equal "/nl/shelves/3", ShelfLinks.noted_shelf
    assert_equal ["nl", 3], ShelfLinks.noted
  end
end
"##,
        )
}

#[test]
#[ignore = "fork spec: routes"]
fn a_route_helper_binds_positionals_in_path_order() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: routes"]
fn a_route_helper_binds_positionals_in_path_order_on_spinel() {
    app().run_spin_test("test/shelves_controller_test.rb").assert_passes();
}

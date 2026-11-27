//! A route helper given fewer positional arguments than its route has
//! segments leaves out the segments its receiver's url options hold and
//! binds the arguments to the rest, as Rails' `handle_positional_args`
//! does: on a class whose singleton includes the url helpers and answers
//! `default_url_options` with the locale `"fr"`, `shelf_path(5)` is
//! `/fr/shelves/5` under `scope "(:locale)"`, from the class's own method
//! and from a test body, and with the locale unset, `/shelves/5`. Ignored
//! as a fork spec until the compiler does; the test passes unchanged in
//! the generated blog on Rails 8.1.4 (`bin/rails test`).

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

    attr_accessor :locale

    def default_url_options
      { locale: locale }
    end

    def shelf(id)
      shelf_path(id)
    end
  end
end
"##,
        )
        .write(
            FILE,
            r##"require "test_helper"

class ShelvesControllerTest < ActionDispatch::IntegrationTest
  teardown { ShelfLinks.locale = nil }

  test "the positional takes the segment the url options do not hold" do
    ShelfLinks.locale = "fr"
    assert_equal "/fr/shelves/5", ShelfLinks.shelf(5)
    assert_equal "/fr/shelves/6", ShelfLinks.shelf_path(6)
    ShelfLinks.locale = nil
    assert_equal "/shelves/5", ShelfLinks.shelf(5)
  end
end
"##,
        )
}

#[test]
#[ignore = "fork spec: routes"]
fn fewer_positionals_take_the_segments_the_url_options_do_not_hold() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: routes"]
fn fewer_positionals_take_the_segments_the_url_options_do_not_hold_on_spinel() {
    app().run_spin_test("test/shelves_controller_test.rb").assert_passes();
}

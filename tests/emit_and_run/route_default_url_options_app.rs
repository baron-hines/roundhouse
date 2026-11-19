//! A route helper called on a class whose singleton includes the url
//! helpers fills a segment the call doesn't give from the class's
//! `default_url_options`, read as the call runs, as Rails' helper merges
//! its `url_options`: from the class's own method, `shelf_path(id: 5)` is
//! `/fr/shelves/5` once the locale is `"fr"`, `/shelves/5` before, and a
//! locale the call gives wins. Ignored as a fork spec until the compiler
//! does; the test passes unchanged in the generated blog on Rails 8.1.4
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

    attr_accessor :locale

    def default_url_options
      { locale: locale }
    end

    def shelf(id)
      shelf_path(id: id)
    end

    def german_shelf(id)
      shelf_path(id: id, locale: "de")
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

  test "the class's default_url_options fill the locale" do
    assert_equal "/shelves/5", ShelfLinks.shelf(5)
    ShelfLinks.locale = "fr"
    assert_equal "/fr/shelves/5", ShelfLinks.shelf(5)
    assert_equal "/de/shelves/5", ShelfLinks.german_shelf(5)
  end
end
"##,
        )
}

#[test]
#[ignore = "fork spec: routes"]
fn a_url_helpers_class_fills_segments_from_its_default_url_options() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: routes"]
fn a_url_helpers_class_fills_segments_from_its_default_url_options_on_spinel() {
    app().run_spin_test("test/shelves_controller_test.rb").assert_passes();
}

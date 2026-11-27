//! A test body's keyword path params fill the route helper's segments, as
//! Rails' url helpers bind them: `shelf_path(locale: "fr", id: 3)` is
//! `/fr/shelves/3` under `scope "(:locale)"`, `shelf_path(id: 3)` is
//! `/shelves/3`, a glob's `file_path(path: "a/b")` is `/files/a/b`, and a
//! class that includes the url helpers binds them the same way. Ignored
//! as a fork spec until the compiler does; the test passes unchanged in
//! the generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/shelves_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            r#"  root "articles#index"

  scope "(:locale)" do
    get "shelves/:id", to: "shelves#show", as: :shelf
    get "files/*path", to: "shelves#file", as: :file
  end
"#,
        )
        .write(
            "app/controllers/shelves_controller.rb",
            r##"class ShelvesController < ApplicationController
  def show
    render plain: "#{params[:locale]}:#{params[:id]}"
  end

  def file
    render plain: "#{params[:locale]}:#{params[:path]}"
  end
end
"##,
        )
        .write(
            "app/models/shelf_links.rb",
            r##"class ShelfLinks
  class << self
    include Rails.application.routes.url_helpers
  end
end
"##,
        )
        .write(
            FILE,
            r##"require "test_helper"

class ShelvesControllerTest < ActionDispatch::IntegrationTest
  test "keyword path params fill the segments they name" do
    assert_equal "/fr/shelves/3", shelf_path(locale: "fr", id: 3)
    assert_equal "/shelves/3", shelf_path(id: 3)
    assert_equal "/files/a/b", file_path(path: "a/b")
    assert_equal "/nl/shelves/7", ShelfLinks.shelf_path(id: 7, locale: "nl")
    get shelf_path(id: 3, locale: "fr")
    assert_equal "fr:3", response.body
  end
end
"##,
        )
}

#[test]
#[ignore = "fork spec: routes"]
fn a_test_bodys_keyword_path_params_fill_the_segments() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: routes"]
fn a_test_bodys_keyword_path_params_fill_the_segments_on_spinel() {
    app().run_spin_test("test/shelves_controller_test.rb").assert_passes();
}

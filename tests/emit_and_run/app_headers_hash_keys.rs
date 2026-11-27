//! A test reads an app object's `headers` Hash with its keys as written:
//! only Rack's headers (the test's `response.headers` and
//! `request.headers`) answer any spelling. A plain object's Hash, read
//! through an ivar or a local, keeps the case its keys were built with.
//! Ignored as a fork spec until the compiler does; the test passes
//! unchanged in the generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/parcels_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "app/models/parcel.rb",
            r#"class Parcel
  def initialize(code)
    @code = code
  end

  def headers
    { "X-Parcel-Code" => @code, "Content-Kind" => "crate" }
  end
end
"#,
        )
        .edit(
            "app/controllers/articles_controller.rb",
            "  def show\n",
            "  def show\n    response.headers[\"X-Parcel-Code\"] = \"shown\"\n",
        )
        .write(
            FILE,
            r#"require "test_helper"

class ParcelsControllerTest < ActionDispatch::IntegrationTest
  setup do
    @parcel = Parcel.new("7")
  end

  test "an app object's headers keep their keys" do
    assert_equal "7", @parcel.headers["X-Parcel-Code"]
    assert_nil @parcel.headers["x-parcel-code"]
    parcel = Parcel.new("8")
    assert_equal "8", parcel.headers["X-Parcel-Code"]
    assert_equal "crate", parcel.headers["Content-Kind"]
    get article_url(articles(:one))
    assert_equal "shown", response.headers["X-Parcel-Code"]
    assert_equal "shown", response.headers["x-parcel-code"]
    assert_equal "7", @parcel.headers["X-Parcel-Code"]
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: controller tests"]
fn an_app_objects_headers_keep_their_keys() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: controller tests"]
fn an_app_objects_headers_keep_their_keys_on_spinel() {
    app().run_spin_test("test/parcels_controller_test.rb").assert_passes();
}

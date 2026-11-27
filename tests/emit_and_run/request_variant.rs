//! `request.variant = :sym` (or an Array of Symbols) and `request.variant`
//! answer as ActionDispatch's MimeNegotiation does: the reader is an
//! ActiveSupport::ArrayInquirer (`phone?`, `any?` with a Symbol or a
//! String, `to_a`), and anything but Symbols raises ArgumentError.
//! Ignored as a fork spec until the compiler does; the test passes
//! unchanged in the generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/kiosks_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/kiosk\", to: \"kiosks#show\"\n",
        )
        .write(
            "app/controllers/kiosks_controller.rb",
            r#"class KiosksController < ApplicationController
  before_action :pick_variant

  def show
    variant = request.variant
    render plain: [variant.phone?, variant.tablet?, variant.any?(:tablet), variant.any?("phone"), variant.to_a.inspect, @rejected].join("|")
  end

  private

  def pick_variant
    case request.headers["X-Screen"]
    when "small"
      request.variant = :phone
    when "both"
      request.variant = [:tablet, :phone]
    end
    begin
      request.variant = "watch" if request.headers["X-Screen"] == "text"
      @rejected = "kept"
    rescue ArgumentError
      @rejected = "rejected"
    end
  end
end
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class KiosksControllerTest < ActionDispatch::IntegrationTest
  test "one variant" do
    get "/kiosk", headers: { "X-Screen" => "small" }
    assert_equal "true|false|false|true|[:phone]|kept", response.body
  end

  test "a list of variants" do
    get "/kiosk", headers: { "X-Screen" => "both" }
    assert_equal "true|true|true|true|[:tablet, :phone]|kept", response.body
  end

  test "no variant, and a String refused" do
    get "/kiosk", headers: { "X-Screen" => "text" }
    assert_equal "false|false|false|false|[]|rejected", response.body
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: controller tests"]
fn request_variant_is_an_array_inquirer() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: controller tests"]
fn request_variant_is_an_array_inquirer_on_spinel() {
    app().run_spin_test("test/kiosks_controller_test.rb").assert_passes();
}

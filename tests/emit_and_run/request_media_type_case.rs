//! A request's media type compares downcased, as Rails' `content_mime_type`
//! looks it up (`Mime::Type.lookup(type.strip.downcase)`): a body sent as
//! `APPLICATION/JSON` or `Application/Json; charset=UTF-8` is parsed as
//! JSON into the params, as a lowercase `application/json` one is. The
//! test file passes unchanged in the generated blog on Rails 8.1.4
//! (`bin/rails test`). The emitted integration harness doesn't parse a
//! raw String body yet under any content type, so the CRuby check posts
//! the same bodies through the served app's dispatch (`Main.dispatch_core`),
//! which parses the lowercase one today. Ignored as a fork spec until the
//! compiler compares it downcased.

use super::emit_and_run;

const FILE: &str = "test/controllers/beacons_controller_test.rb";

const SCRIPT: &str = r##"
require "stringio"
[["application/json", "rust"], ["APPLICATION/JSON", "amber"], ["Application/Json; charset=UTF-8", "teal"]].each do |type, signal|
  body = "{\"signal\":\"#{signal}\"}"
  env = {
    "REQUEST_METHOD" => "POST", "PATH_INFO" => "/beacons", "QUERY_STRING" => "",
    "CONTENT_TYPE" => type, "CONTENT_LENGTH" => body.bytesize.to_s, "HTTP_ACCEPT" => "text/plain"
  }
  status, text = Main.dispatch_core(env, StringIO.new(body))
  raise "#{type}: expected 200 and #{signal.inspect}, got #{status} #{text.inspect}" unless status == 200 && text == signal
end
puts "media types passed"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  post \"/beacons\", to: \"beacons#create\"\n",
        )
        .write(
            "app/controllers/beacons_controller.rb",
            r#"class BeaconsController < ApplicationController
  skip_forgery_protection

  def create
    render plain: params[:signal].to_s
  end
end
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class BeaconsControllerTest < ActionDispatch::IntegrationTest
  test "a lowercase json type is parsed as json" do
    post "/beacons", params: '{"signal":"rust"}', headers: { "CONTENT_TYPE" => "application/json" }
    assert_equal "rust", response.body
  end

  test "an upper-case json type is parsed as json" do
    post "/beacons", params: '{"signal":"amber"}', headers: { "CONTENT_TYPE" => "APPLICATION/JSON" }
    assert_response :success
    assert_equal "amber", response.body
  end

  test "a mixed-case json type with a charset is parsed as json" do
    post "/beacons", params: '{"signal":"teal"}', headers: { "CONTENT_TYPE" => "Application/Json; charset=UTF-8" }
    assert_response :success
    assert_equal "teal", response.body
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: controller tests"]
fn a_request_media_type_compares_downcased() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("media types passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: controller tests"]
fn a_request_media_type_compares_downcased_on_spinel() {
    app().run_spin_test("test/beacons_controller_test.rb").assert_passes();
}

//! `fixture_file_upload` answers a Rack::Test::UploadedFile read from
//! `test/fixtures/files`, and a functional request carrying one is sent
//! multipart even with `as: :json`, as ActionDispatch's TestRequest does
//! (`should_multipart?` before the content type): the controller reads the
//! file as an ActionDispatch::Http::UploadedFile, the other params as form
//! Strings, and the request's format stays JSON. Ignored as a fork spec
//! until the compiler does; the test passes unchanged in the generated
//! blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/scans_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  resources :scans, only: :create\n",
        )
        .write("test/fixtures/files/manifest.txt", "crate 12\n")
        .write(
            "app/controllers/scans_controller.rb",
            r#"class ScansController < ApplicationController
  skip_forgery_protection

  def create
    file = params[:sheet]
    render json: {
      "kind" => file.class.name,
      "name" => file.original_filename,
      "type" => file.content_type,
      "text" => file.read,
      "copies" => params[:copies],
      "format" => request.format.symbol.to_s,
      "multipart" => request.media_type
    }
  end
end
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class ScansControllerTest < ActionController::TestCase
  test "fixture_file_upload answers a rack-test upload" do
    upload = fixture_file_upload("manifest.txt", "text/plain")
    assert_kind_of Rack::Test::UploadedFile, upload
    assert_equal "manifest.txt", upload.original_filename
    assert_equal "text/plain", upload.content_type
    assert_equal "crate 12\n", upload.read
  end

  test "an upload sends the request multipart, even as json" do
    post :create, params: { sheet: fixture_file_upload("manifest.txt", "text/plain"), copies: 3 }, as: :json
    assert_response :success
    body = JSON.parse(response.body)
    assert_equal "ActionDispatch::Http::UploadedFile", body["kind"]
    assert_equal "manifest.txt", body["name"]
    assert_equal "text/plain", body["type"]
    assert_equal "crate 12\n", body["text"]
    assert_equal "3", body["copies"]
    assert_equal "json", body["format"]
    assert_equal "multipart/form-data", body["multipart"]
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: controller tests"]
fn fixture_file_upload_sends_a_multipart_request() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: controller tests"]
fn fixture_file_upload_sends_a_multipart_request_on_spinel() {
    app().run_spin_test("test/scans_controller_test.rb").assert_passes();
}

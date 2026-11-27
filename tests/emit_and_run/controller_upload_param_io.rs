//! A controller test's upload param reaches the controller as an uploaded
//! file that reads as an IO, as Rails' multipart round trip hands it: the
//! action can `rewind`, `read` and ask `eof?`, and reads the bytes, not the
//! upload's `to_s`. Ignored as a fork spec until the compiler does; the
//! test passes unchanged in the generated blog on Rails 8.1.4
//! (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/stamps_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  resources :stamps, only: :create\n",
        )
        .write("test/fixtures/files/stamp.txt", "inked\n")
        .write(
            "app/controllers/stamps_controller.rb",
            r#"class StampsController < ApplicationController
  skip_forgery_protection

  def create
    upload = params[:artwork]
    first = upload.read
    at_end = upload.eof?
    upload.rewind
    again = upload.read
    render plain: [upload.class.name, first.inspect, at_end, again.inspect, upload.size].join("|")
  end
end
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class StampsControllerTest < ActionController::TestCase
  test "an upload param reads as an IO" do
    post :create, params: { artwork: fixture_file_upload("stamp.txt", "text/plain") }
    assert_response :success
    assert_equal "ActionDispatch::Http::UploadedFile|\"inked\\n\"|true|\"inked\\n\"|6", response.body
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: controller tests"]
fn an_upload_param_reads_as_an_io() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: controller tests"]
fn an_upload_param_reads_as_an_io_on_spinel() {
    app().run_spin_test("test/stamps_controller_test.rb").assert_passes();
}

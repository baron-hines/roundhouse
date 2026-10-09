//! campfire's SQLite-observer caches on the CRuby overlay; the shared
//! contract and its native twin are described in
//! `tests/support/campfire_caches.rs`.

use super::campfire_caches_contract::{self as contract, Contract};
use super::emit_and_run;

fn assert_runs(contract: &Contract) {
    let run = contract.overlay().run_ruby(contract.script);
    run.assert_passes();
    assert_eq!(run.stdout, contract.expected, "stderr:\n{}", run.stderr);
}

#[test]
fn nested_multi_write_destructures_each_group() {
    assert_runs(&contract::NESTED_MULTI_WRITE);
}

#[test]
fn data_define_block_methods_belong_to_the_data_class() {
    assert_runs(&contract::DATA_BLOCK_METHODS);
}

#[test]
fn response_helpers_run_as_rails_runs_them() {
    assert_runs(&contract::RESPONSE_HELPERS);
}

/// The same helpers where campfire calls them — a controller concern,
/// the strictly typed context the errors were reported in: negotiate
/// the encoding off the request header, gzip the body with a weak ETag,
/// snapshot the session, and key the page by its request facts.
#[test]
fn response_helpers_run_in_a_controller_concern() {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/cached\", to: \"cached_pages#show\"\n",
        )
        .write(
            "app/controllers/concerns/page_reuse.rb",
            r#"require "zlib"

module PageReuse
  extend ActiveSupport::Concern

  private
    def negotiated_encoding
      Rack::Utils.select_best_encoding(%w[ gzip identity ], Rack::Utils.q_values(request.headers["Accept-Encoding"]))
    end

    def page_key(encoding)
      ActiveSupport::JSON.encode([ controller_path, request.fullpath, encoding, session.to_hash.except("_csrf_token") ])
    end
end
"#,
        )
        .write(
            "app/controllers/cached_pages_controller.rb",
            r##"class CachedPagesController < ApplicationController
  include PageReuse

  def show
    session[:visits] = "1"
    snapshot = session.to_hash.deep_dup
    encoding = negotiated_encoding
    html = "<p>cached page</p>"
    body = encoding == "gzip" ? Zlib.gzip(html) : html
    response.headers["ETag"] = %(W/"#{Digest::SHA256.hexdigest(body).byteslice(0, 32)}")
    response.headers["X-Page-Key"] = page_key(encoding)
    response.headers["X-Snapshot-Same"] = (snapshot == session.to_hash).to_s
    response.headers["Content-Encoding"] = "gzip" if encoding == "gzip"
    render plain: body
  end
end
"##,
        )
        .write(
            "test/controllers/cached_pages_controller_test.rb",
            r#"require "test_helper"

class CachedPagesControllerTest < ActionDispatch::IntegrationTest
  test "gzip is negotiated and keyed" do
    get "/cached", headers: { "Accept-Encoding" => "br, gzip;q=0.5" }
    assert_response :success
    assert_equal "gzip", response.headers["Content-Encoding"]
    assert_equal "<p>cached page</p>", Zlib.gunzip(response.body)
    assert_match(/\AW\/"[0-9a-f]{32}"\z/, response.headers["ETag"])
    assert_equal "true", response.headers["X-Snapshot-Same"]
    key = ActiveSupport::JSON.decode(response.headers["X-Page-Key"])
    assert_equal [ "cached_pages", "/cached", "gzip" ], key.first(3)
    assert_equal "1", key.last["visits"]
  end

  test "a refused identity and no acceptable encoding answers nil" do
    get "/cached", headers: { "Accept-Encoding" => "gzip;q=0, identity;q=0" }
    assert_response :success
    assert_nil response.headers["Content-Encoding"]
    assert_equal "<p>cached page</p>", response.body
    assert_nil ActiveSupport::JSON.decode(response.headers["X-Page-Key"])[2]
  end
end
"#,
        )
        .run_test("test/controllers/cached_pages_controller_test.rb")
        .assert_passes();
}

#[test]
fn sqlite_observer_and_checkpointer_surface_runs() {
    assert_runs(&contract::SQLITE_OBSERVER);
}

#[test]
fn record_snapshots_and_the_bounded_store_run() {
    assert_runs(&contract::RECORD_SNAPSHOT);
}

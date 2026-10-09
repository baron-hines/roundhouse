//! The ActionController::Head contract over the emitted Spinel HTTP server.

use super::{emit_and_run, native_http};
use roundhouse::project::BuildTarget;

#[test]
#[ignore = "requires Spinel; run in its CI lane"]
fn head_options_are_observable_in_the_native_http_response() {
    let overlay = emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/head-probe\", to: \"head_probes#created\"\n  get \"/head-probe/json\", to: \"head_probes#json\"\n  get \"/head-probe/negotiated\", to: \"head_probes#negotiated\"\n  get \"/head-probe/empty\", to: \"head_probes#empty\"\n  get \"/head-probe/reset\", to: \"head_probes#reset\"\n  get \"/head-probe/not-modified\", to: \"head_probes#not_modified\"\n  get \"/head-probe/defaulted\", to: \"head_probes#defaulted\"\n  get \"/head-probe/model-location\", to: \"head_probes#model_location\"\n",
        )
        .write(
            "app/controllers/head_probes_controller.rb",
            r#"class HeadProbesController < ApplicationController
  def created
    head :created, { "x-custom_header" => 17, location: "/head-probe/created", content_type: "text/plain; charset=utf-8" }
  end

  def json
    head :ok, content_type: :json
  end

  def negotiated
    head :ok
  end

  def empty
    head :no_content
  end

  def reset
    head :reset_content
  end

  def not_modified
    head :not_modified
  end

  def defaulted
    head nil
  end

  def model_location
    @article = Article.create(title: "Head location", body: "A body long enough for validation.")
    head :created, location: @article
  end
end
"#,
        );
    let (tree, errors) = overlay.emit(BuildTarget::Spinel);
    assert!(
        errors.is_empty(),
        "analysis/emission errors:\n{}",
        errors.join("\n")
    );

    native_http::build(&tree);
    let server = native_http::Server::start(&tree);

    let created = server.get("/head-probe");
    assert_eq!(created.status, 201, "{}", server.log());
    assert_eq!(
        created.headers.get("x-custom-header").map(String::as_str),
        Some("17")
    );
    assert_eq!(
        created.headers.get("location").map(String::as_str),
        Some("/head-probe/created")
    );
    assert_eq!(
        created.headers.get("content-type").map(String::as_str),
        Some("text/plain")
    );
    assert!(created.body.is_empty(), "body was {:?}", created.body);

    let json = server.get("/head-probe/json");
    assert_eq!(json.status, 200, "{}", server.log());
    assert_eq!(
        json.headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
    assert!(json.body.is_empty(), "body was {:?}", json.body);

    let negotiated = server.get("/head-probe/negotiated.xml");
    assert_eq!(negotiated.status, 200, "{}", server.log());
    assert_eq!(
        negotiated.headers.get("content-type").map(String::as_str),
        Some("application/xml")
    );
    assert!(negotiated.body.is_empty(), "body was {:?}", negotiated.body);

    let empty = server.get("/head-probe/empty");
    assert_eq!(empty.status, 204, "{}", server.log());
    assert!(
        !empty.headers.contains_key("content-type"),
        "headers were {:?}",
        empty.headers
    );
    assert!(empty.body.is_empty(), "body was {:?}", empty.body);

    let reset = server.get("/head-probe/reset");
    assert_eq!(reset.status, 205, "{}", server.log());
    assert!(
        !reset.headers.contains_key("content-type"),
        "headers were {:?}",
        reset.headers
    );
    assert!(reset.body.is_empty(), "body was {:?}", reset.body);

    let not_modified = server.get("/head-probe/not-modified");
    assert_eq!(not_modified.status, 304, "{}", server.log());
    assert!(
        !not_modified.headers.contains_key("content-type"),
        "headers were {:?}",
        not_modified.headers
    );
    assert!(
        not_modified.body.is_empty(),
        "body was {:?}",
        not_modified.body
    );

    let defaulted = server.get("/head-probe/defaulted");
    assert_eq!(defaulted.status, 200, "{}", server.log());
    assert_eq!(
        defaulted.headers.get("content-type").map(String::as_str),
        Some("text/html")
    );
    assert!(defaulted.body.is_empty(), "body was {:?}", defaulted.body);

    let model_location = server.get("/head-probe/model-location");
    assert_eq!(model_location.status, 201, "{}", server.log());
    assert_eq!(
        model_location.headers.get("location").map(String::as_str),
        Some("/articles/1")
    );
    assert!(
        model_location.body.is_empty(),
        "body was {:?}",
        model_location.body
    );
}

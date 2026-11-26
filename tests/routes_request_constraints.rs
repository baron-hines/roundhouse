//! A `constraints` Hash keyed by a request attribute guards its routes.
//!
//! An app's services routes sit inside
//! `constraints host: Rails.configuration.app_host_constraint,
//! protocol: "https://" do … end`. `host:` and `protocol:` name no path
//! segment: Rails asks the request (`request.host`, `request.protocol`),
//! even for a literal value (`subdomain: "api"`). The Ruby-family router
//! asks it as it asks a guard object (`route_request_guards.rs`); a router
//! that matches by verb and path alone refuses the routes, since serving
//! them answered requests for any host. A segment constraint still
//! refuses a value that is no literal.

use roundhouse::App;
use roundhouse::ingest::ingest_routes;
use roundhouse::lower::routes::{FlatRoute, flatten_routes, flatten_routes_for_helpers};

fn routes(source: &str) -> Result<Vec<FlatRoute>, String> {
    let mut app = App::default();
    app.routes = ingest_routes(
        format!("Rails.application.routes.draw do\n{source}\nend\n").as_bytes(),
        "config/routes.rb",
    )
    .map_err(|e| e.to_string())?;
    Ok(flatten_routes(&app))
}

fn app(source: &str) -> App {
    let mut app = App::default();
    app.routes = ingest_routes(format!("Rails.application.routes.draw do\n{source}\nend\n").as_bytes(), "config/routes.rb")
        .expect("ingest routes");
    app
}

/// The constraints a target whose router matches by verb and path alone refuses; the Ruby family asks them.
fn refusals(app: &App) -> usize {
    assert!(roundhouse::lower::route_guards::route_guard_gaps_for_target(app, roundhouse::project::BuildTarget::Ruby).is_empty());
    roundhouse::lower::route_guards::route_guard_gaps_for_target(app, roundhouse::project::BuildTarget::Rust).len()
}

#[test]
fn a_host_and_protocol_block_refuses_its_routes() {
    let app = app(
        r#"constraints host: Rails.configuration.app_host_constraint, protocol: "https://" do
  scope :services, module: :services do
    scope :connected_paypal do
      get "partner_referral/:shop_id", controller: "partner_referral", action: :create, as: :partner_referral
    end
  end
end
get "/ping", to: "pings#show""#,
    );
    let paths: Vec<String> = flatten_routes(&app).iter().map(|route| route.path.clone()).collect();
    assert_eq!(paths, ["/ping"], "{paths:?}");
    assert!(flatten_routes_for_helpers(&app).iter().any(|route| route.as_name == "partner_referral"));
    assert_eq!(refusals(&app), 1, "{:#?}", app.routes.diagnostics);
}

#[test]
fn a_literal_subdomain_on_a_route_or_a_scope_refuses_it_and_keeps_its_segment_constraints_apart() {
    let app = app(
        r#"get "/things/:id", to: "things#show", constraints: { subdomain: "api", id: /\d+/ }
scope constraints: ->(req) { req.subdomain == "admin" } do
  get "/reports", to: "reports#index"
end
get "/other/:id", to: "others#show", constraints: { id: /\d+/ }"#,
    );
    let paths: Vec<String> = flatten_routes(&app).iter().map(|route| route.path.clone()).collect();
    assert_eq!(paths, ["/other/:id"], "{paths:?}");
    assert_eq!(refusals(&app), 2, "{:#?}", app.routes.diagnostics);
}

#[test]
fn a_segment_constraint_that_is_no_literal_stays_a_gap() {
    let err = routes(
        r#"constraints id: Rails.configuration.id_pattern do
  get "/things/:id", to: "things#show"
end"#,
    )
    .expect_err("a segment constraint");
    assert!(err.contains("is not a literal regex or a plain name"), "{err}");
}

//! A `root` inside an optional scope (`scope "(/locale/:locale)" do root … end`)
//! answers each expansion, as Rails' one route does, and its helper leaves the
//! optional segment out unless given. The path kept the group verbatim
//! (`/(/locale/:locale)`), which no request reaches.

use roundhouse::App;
use roundhouse::ingest::ingest_routes;
use roundhouse::lower::routes::{flatten_routes, flatten_routes_for_helpers};

#[test]
fn a_root_in_an_optional_scope_answers_with_and_without_the_segment() {
    let mut app = App::default();
    app.routes = ingest_routes(
        b"Rails.application.routes.draw do\n  scope \"(/locale/:locale)\" do\n    root \"stories#index\"\n  end\nend\n",
        "config/routes.rb",
    )
    .expect("ingest routes");
    let paths: Vec<String> = flatten_routes(&app).iter().map(|r| r.path.clone()).collect();
    assert_eq!(paths, ["/locale/:locale", "/"], "{paths:?}");
    let named: Vec<(String, String, usize)> = flatten_routes_for_helpers(&app)
        .into_iter()
        .filter(|r| r.named)
        .map(|r| (r.as_name, r.path, r.required_params))
        .collect();
    assert_eq!(named, [("root".to_string(), "/locale/:locale".to_string(), 0)], "{named:?}");
}

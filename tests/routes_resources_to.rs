//! `resources :articles, only: [:index], to: "trends#articles"` inside
//! `resources :trends`: Rails passes `to:` to each route the resource draws,
//! so `GET /trends/:trend_id/articles` is `trends#articles`. The option was
//! ignored and the route went to `articles#index`, which forem's API does not
//! define for that path.

use roundhouse::App;
use roundhouse::ingest::ingest_routes;
use roundhouse::lower::routes::flatten_routes;

fn routes(source: &str) -> Vec<(String, String, String, String)> {
    let mut app = App::default();
    app.routes = ingest_routes(format!("Rails.application.routes.draw do\n{source}\nend\n").as_bytes(), "config/routes.rb")
        .expect("ingest routes");
    flatten_routes(&app)
        .into_iter()
        .map(|r| (format!("{:?}", r.method), r.path, format!("{}#{}", r.controller.0.as_str(), r.action.as_str()), r.as_name))
        .collect()
}

#[test]
fn a_resources_to_sends_every_route_it_draws_to_that_action() {
    let drawn = routes(
        r#"resources :trends, only: %i[show], param: :id_or_slug do
  resources :articles, only: [:index], to: "trends#articles"
end"#,
    );
    let nested = drawn.iter().find(|(_, path, _, _)| path == "/trends/:trend_id_or_slug/articles").expect("the nested route");
    assert_eq!(nested.2, "TrendsController#articles", "{drawn:#?}");
    assert_eq!(nested.3, "trend_articles", "the helper still names the resource");
}

#[test]
fn a_resources_to_that_is_no_literal_is_refused() {
    let mut app = App::default();
    let err = ingest_routes(b"Rails.application.routes.draw do\n  resources :articles, to: TARGET\nend\n", "config/routes.rb")
        .map(|routes| app.routes = routes)
        .expect_err("a dynamic `to:`");
    assert!(err.to_string().contains("`to:` is not a literal"), "{err}");
}

/// `get "/#{ENV.fetch("BILLBOARD_URL_COMPONENT", "bb")}/:placement_area"` inside
/// `scope "/:username/:slug"`: a path ingest cannot read is refused, not drawn at the
/// scope's own path, where it answered every `/:username/:slug` before the article route.
#[test]
fn an_interpolated_route_path_is_refused_rather_than_drawn_at_its_scope() {
    let err = ingest_routes(
        b"Rails.application.routes.draw do\n  scope \"/:username/:slug\" do\n    get \"/#{ENV.fetch('X', 'bb')}/:placement_area\", to: \"billboards#show\"\n  end\nend\n",
        "config/routes.rb",
    )
    .expect_err("a path that is not a literal");
    assert!(err.to_string().contains("route path is not a literal string"), "{err}");
}

/// The hashrocket spelling (`get "#{root_path}/x" => "c#a"`) reads a path a loop binds, as the
/// positional one does, and refuses one it cannot read rather than dropping the route.
#[test]
fn an_interpolated_hashrocket_path_is_read_from_its_loop_or_refused() {
    let mut app = App::default();
    app.routes = ingest_routes(
        b"Rails.application.routes.draw do\n  %w[users u].each do |root_path|\n    post \"#{root_path}/confirm-session\" => \"users#confirm_session\"\n  end\nend\n",
        "config/routes.rb",
    )
    .expect("ingest routes");
    let paths: Vec<String> = flatten_routes(&app).into_iter().map(|r| r.path).collect();
    assert_eq!(paths, ["/users/confirm-session", "/u/confirm-session"], "{paths:?}");
    let err = ingest_routes(b"Rails.application.routes.draw do\n  get \"/#{ENV.fetch('X', 'bb')}/:id\" => \"billboards#show\"\nend\n", "config/routes.rb")
        .expect_err("a path that is not a literal");
    assert!(err.to_string().contains("route path is not a literal string"), "{err}");
}

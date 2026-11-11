//! A request constraint, a lambda or an object Rails asks with the
//! request (`constraints: ->(req) { req.format == :json }`,
//! `constraints(SomeGuard.new) do … end`), guards its routes. The
//! Ruby-family router asks it once the path matches
//! (`RoundhouseRouteGuards`), and a request it refuses reaches the routes
//! after it. A target whose router matches by verb and path alone refuses
//! the routes instead, with an error where the constraint is written:
//! serving them answered every request the constraint refuses, which left
//! a later route on the same path unreachable (#727). A constraint no
//! router can ask is refused at ingest.

use roundhouse::App;
use roundhouse::ingest::ingest_routes;
use roundhouse::lower::route_guards::{guard_class, route_guard_gaps_for_target};
use roundhouse::lower::routes::{flatten_routes, flatten_routes_asking_guards, flatten_routes_for_helpers};
use roundhouse::project::BuildTarget;

fn app(source: &str) -> App {
    let mut app = App::default();
    app.routes = ingest_routes(format!("Rails.application.routes.draw do\n{source}\nend\n").as_bytes(), "config/routes.rb")
        .expect("ingest routes");
    app
}

fn strict_refusals(app: &App, target: BuildTarget) -> usize {
    route_guard_gaps_for_target(app, target).iter().filter(|d| d.severity == roundhouse::diagnostic::Severity::Error).count()
}

#[test]
fn a_lambda_constraint_is_asked_by_the_ruby_family_and_refused_elsewhere() {
    let app = app(
        r#"get "/invite/:code", to: "widgets#show", constraints: ->(req) { req.format == :json }
get "/invite/:code", to: "gadgets#new""#,
    );
    let asked: Vec<(String, Vec<usize>)> =
        flatten_routes_asking_guards(&app).into_iter().map(|(r, guards)| (r.controller.0.as_str().to_string(), guards)).collect();
    assert_eq!(asked, [("WidgetsController".to_string(), vec![0]), ("GadgetsController".to_string(), vec![])], "{asked:?}");
    let (class, rbs) = guard_class(&app.routes.entries).expect("a guard method");
    assert_eq!(class.methods.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["guard_0"]);
    assert!(rbs.contains("def self.guard_0: (ActionDispatch::Request request) -> bool"), "{rbs}");
    assert_eq!(class.methods[0].params[0].name.as_str(), "req");
    assert!(!app.routes.diagnostics.iter().any(|d| d.message.contains("request constraint")), "{:#?}", app.routes.diagnostics);
    let dispatched: Vec<String> = flatten_routes(&app).iter().map(|r| r.controller.0.as_str().to_string()).collect();
    assert_eq!(dispatched, ["GadgetsController"], "{dispatched:?}");
    assert_eq!(strict_refusals(&app, BuildTarget::Rust), 1);
    assert_eq!(strict_refusals(&app, BuildTarget::Spinel), 0);
}

#[test]
fn a_guard_object_block_is_asked_with_matches_and_keeps_its_helpers() {
    let app = app(
        r#"constraints(AdminGuard.new) do
  get "/reports", to: "reports#index", as: :reports
  resources :audits, only: :index
end
get "/ping", to: "pings#show""#,
    );
    let asked: Vec<(String, Vec<usize>)> = flatten_routes_asking_guards(&app).into_iter().map(|(r, guards)| (r.path, guards)).collect();
    assert_eq!(asked, [("/reports".to_string(), vec![0]), ("/audits".to_string(), vec![0]), ("/ping".to_string(), vec![])], "{asked:?}");
    let (class, _) = guard_class(&app.routes.entries).expect("a guard method");
    assert_eq!(roundhouse::emit::ruby::emit_expr(&class.methods[0].body), "AdminGuard.new.matches?(request)");
    let helpers: Vec<String> = flatten_routes_for_helpers(&app).iter().map(|r| r.as_name.clone()).collect();
    assert!(helpers.iter().any(|h| h == "reports") && helpers.iter().any(|h| h == "audits"), "{helpers:?}");
    assert_eq!(strict_refusals(&app, BuildTarget::Crystal), 1, "one error at the constraint");
}

/// A constraint whose expression does not ingest (`->(req, &block) { … }`) is refused where it is
/// written all the same: it is never run, so the rest of the table and the route's helper stand.
#[test]
fn a_constraint_that_does_not_ingest_is_still_refused_in_place() {
    let app = app(
        r#"get "/reports", to: "reports#index", as: :reports, constraints: ->(req, &block) { true }
get "/ping", to: "pings#show""#,
    );
    let flat = flatten_routes(&app);
    let dispatched: Vec<&str> = flat.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(dispatched, ["/ping"], "{dispatched:?}");
    assert!(flatten_routes_for_helpers(&app).iter().any(|r| r.as_name == "reports"));
    let error = app.routes.diagnostics.iter().find(|d| d.message.contains("request constraint")).expect("located refusal");
    assert!(!error.span.is_synthetic(), "{error:?}");
}

#[test]
fn constraints_that_do_not_ingest_while_surveying_keep_their_own_places() {
    roundhouse::ingest::survey::activate();
    let app = app(
        r#"get "/reports", to: "reports#index", constraints: ->(req, &block) { true }
get "/audits", to: "audits#index", constraints: ->(req, &block) { true }"#,
    );
    roundhouse::ingest::survey::drain();
    let spans: Vec<_> = app.routes.diagnostics.iter().filter(|d| d.message.contains("request constraint")).map(|d| d.span).collect();
    assert_eq!(spans.len(), 2, "{:#?}", app.routes.diagnostics);
    assert!(spans.iter().all(|s| !s.is_synthetic()) && spans[0] != spans[1], "{spans:?}");
}

/// The Ruby-family table keeps a route's segment requirements beside its constraints' chain, and a
/// route with requirements and no constraint keeps them too.
#[test]
fn a_guarded_routes_table_entry_keeps_its_segment_requirements() {
    let app = app(
        r#"get "/files/:name", to: "files#show", constraints: { name: /[a-z]+/, subdomain: "cdn" }
get "/files/:name", to: "files#index", constraints: { name: /[a-z]+/ }"#,
    );
    let table = roundhouse::lower::routes_to_library::lower_routes_to_guarded_dispatch_functions(&app)
        .into_iter()
        .find(|f| f.name.as_str() == "table")
        .expect("RouteTable.table");
    let source = roundhouse::emit::ruby::emit_expr(&table.body);
    let requirement = r#""4.name33.C+0026.abcdefghijklmnopqrstuvwxyz""#;
    assert!(source.contains(&format!(r#"Route.new("GET", "/files/:name", :files, :show, nil, "", {requirement}, 0)"#)), "{source}");
    assert!(source.contains(&format!(r#"Route.new("GET", "/files/:name", :files, :index, nil, "", {requirement})"#)), "{source}");
}

/// A Rack mount after a guarded route is chosen before the router asks the constraint
/// (`RouteTable.rack_index`): the Ruby family refuses that route too, with an error at the constraint.
#[test]
fn a_constraint_before_a_rack_mount_is_refused_on_the_ruby_family_too() {
    let mut app = app(r#"get "/files/:name", to: "files#show", constraints: ->(req) { req.subdomain == "cdn" }"#);
    app.routes.entries.push(roundhouse::dialect::RouteSpec::RackMount {
        target: roundhouse::ClassId(roundhouse::ident::Symbol::from("StaticFiles")),
        path: "/files".into(),
        span: roundhouse::span::Span::synthetic(),
    });
    assert!(flatten_routes_asking_guards(&app).is_empty(), "the guarded route is not asked");
    let errors = route_guard_gaps_for_target(&app, BuildTarget::Spinel);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].message.contains("Rack mount"), "{errors:#?}");
    assert!(route_guard_gaps_for_target(&app, BuildTarget::Blog).is_empty());
}

/// A split draw (`draw :files`) holds a guarded route and a later mount in one transparent scope.
#[test]
fn a_constraint_before_a_mount_in_the_same_drawn_file_is_refused_too() {
    let mut app = app(r#"get "/files/:name", to: "files#show", constraints: ->(req) { req.subdomain == "cdn" }"#);
    let mut entries = std::mem::take(&mut app.routes.entries);
    entries.push(roundhouse::dialect::RouteSpec::RackMount {
        target: roundhouse::ClassId(roundhouse::ident::Symbol::from("StaticFiles")),
        path: "/files".into(),
        span: roundhouse::span::Span::synthetic(),
    });
    app.routes.entries.push(roundhouse::dialect::RouteSpec::Scope {
        path: None,
        module: None,
        as_prefix: None,
        defaults: Default::default(),
        nest: false,
        suppress_helpers: false,
        mount: false,
        within: roundhouse::dialect::ResourceScope::Nested,
        entries,
    });
    assert!(flatten_routes_asking_guards(&app).is_empty(), "the guarded route is not asked");
    assert_eq!(route_guard_gaps_for_target(&app, BuildTarget::Ruby).len(), 1);
}

/// A mount whose prefix no guarded route can fall under does not refuse the constraint.
#[test]
fn a_constraint_before_a_mount_elsewhere_is_still_asked() {
    let mut app = app(r#"get "/reports", to: "reports#index", constraints: ->(req) { req.subdomain == "admin" }"#);
    app.routes.entries.push(roundhouse::dialect::RouteSpec::RackMount {
        target: roundhouse::ClassId(roundhouse::ident::Symbol::from("StaticFiles")),
        path: "/assets".into(),
        span: roundhouse::span::Span::synthetic(),
    });
    assert_eq!(flatten_routes_asking_guards(&app).len(), 1, "the guarded route is asked");
    assert!(route_guard_gaps_for_target(&app, BuildTarget::Ruby).is_empty());
}

/// `lambda { |req| ... }` and `proc { |req| ... }` are callables like
/// `->(req) { ... }`. Neither answers `matches?`.
#[test]
fn a_kernel_lambda_or_proc_constraint_is_asked_as_a_callable() {
    for form in ["lambda", "proc"] {
        let app = app(&format!(r#"get "/invite/:code", to: "widgets#show", constraints: {form} {{ |req| req.format == :json }}"#));
        let (class, _) = guard_class(&app.routes.entries).expect("a guard method");
        assert_eq!(class.methods[0].params[0].name.as_str(), "req", "{form}");
        let body = format!("{:?}", class.methods[0].body);
        assert!(!body.contains("matches?"), "{form}: {body}");
    }
}

/// A constraint that reads a local of the routes file can't move into the
/// generated guard method, where that local isn't bound. It is refused
/// where it is written, like any other constraint the router can't ask.
#[test]
fn a_constraint_reading_a_local_of_the_routes_file_is_refused_in_place() {
    for constraint in ["->(req) { req.path == name }", "lambda { |req| req.path == name }", "proc { |req| [name].include?(req.path) }",
        // Bound only if the branch runs.
        r#"->(req) { name = "/other" if req.path == "/other"; req.path == name }"#,
        r#"->(req) { req.path == "/other" && (name = "/other"); req.path == name }"#,
        // A compound assignment reads its target first.
        r#"->(req) { name += "/x"; req.path == name }"#,
        r#"->(req) { name ||= "/x"; req.path == name }"#,
        // A nested default runs inside the generated method.
        r#"->(req) { (->(x = name) { x }).call == req.path }"#,
    ] {
        let app = app(&format!(
            r#"name = "/admin"
get "/x", to: "a#b", as: :x, constraints: {constraint}
get "/ping", to: "pings#show""#
        ));
        let flat = flatten_routes(&app);
        let dispatched: Vec<&str> = flat.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(dispatched, ["/ping"], "{constraint}: {dispatched:?}");
        assert!(flatten_routes_for_helpers(&app).iter().any(|r| r.as_name == "x"), "{constraint}");
        let error = app.routes.diagnostics.iter().find(|d| d.message.contains("request constraint")).expect("located refusal");
        assert!(!error.span.is_synthetic(), "{error:?}");
    }
}

/// What the constraint binds itself stays askable: its parameter, its
/// own assignment, and a nested block's parameter.
#[test]
fn a_constraint_reading_only_what_it_binds_is_asked() {
    for constraint in [
        r#"->(req) { prefix = "/a"; [prefix].any? { |p| req.path.start_with?(p) } }"#,
        r#"->(req) { n = 0; n += 1; n == req.path.size }"#,
        r#"->(req) { (->(a, b = a) { b }).call(req.path) == "/x" }"#,
    ] {
        let app = app(&format!(r#"get "/x", to: "a#b", constraints: {constraint}"#));
        assert!(guard_class(&app.routes.entries).is_some(), "{constraint}");
        assert!(!app.routes.diagnostics.iter().any(|d| d.message.contains("request constraint")), "{constraint}: {:#?}", app.routes.diagnostics);
    }
}

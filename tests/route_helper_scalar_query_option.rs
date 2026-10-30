//! A boolean, an Integer that can be nil, or a Float under a route
//! helper's query option reaches the helper as text, the `String?` the
//! helper's option is typed. A nil stays nil, so the helper leaves the
//! key out. Before, only a value typed exactly `Integer` went through
//! `to_s`; the rest reached the helper as they were and contradicted
//! its signature under Spinel's `--rbs .`.
//!
//! Expected paths are what Rails 8.1.4's generated helper writes for the
//! same calls (`widgets_path(featured: true)` is
//! `/widgets?featured=true`, `widgets_path(page: nil)` is `/widgets`).
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;

const CONTROLLER: &str = "class WidgetsController < ApplicationController
  def index
    render json: query_paths(2)
  end

  def query_paths(n)
    [
      widgets_path(featured: true),
      widgets_path(featured: false),
      widgets_path(page: n.to_i.positive? ? n.to_i : nil),
      widgets_path(page: (n.to_i - 2).positive? ? n.to_i : nil),
      widgets_path(ratio: 1.5)
    ]
  end
end
";

const ASSERTIONS: &str = r#"
require_relative "app/controllers/widgets_controller"

paths = WidgetsController.new.query_paths(2)
expected = ["/widgets?featured=true", "/widgets?featured=false", "/widgets?page=2", "/widgets", "/widgets?ratio=1.5"]
raise paths.inspect unless paths == expected
puts "query paths passed"
"#;

fn example() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema.define do\nend\n")
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("app/controllers/widgets_controller.rb", CONTROLLER)
        .write("config/routes.rb", "Rails.application.routes.draw do\n  resources :widgets, only: :index\nend\n")
}

#[test]
fn a_scalar_query_option_reaches_the_helper_as_text() {
    let (emitted, _) = example().emit(BuildTarget::Ruby);
    let controller = std::fs::read_to_string(emitted.join("app/controllers/widgets_controller.rb")).expect("read the emitted controller");
    for call in [
        "RouteHelpers.widgets_path(featured: true.to_s)",
        "RouteHelpers.widgets_path(featured: false.to_s)",
        "RouteHelpers.widgets_path(ratio: 1.5.to_s)",
    ] {
        assert!(controller.contains(call), "{call}:\n{controller}");
    }
    // The Integer that can be nil is bound once and keeps its nil.
    for n in 0..2 {
        assert!(
            controller.contains(&format!("if route_segment_{n}.nil?")) && controller.contains(&format!("route_segment_{n}.to_s")),
            "route_segment_{n}:\n{controller}"
        );
    }
}

#[test]
fn a_scalar_query_option_runs_in_ruby() {
    example().run_ruby(ASSERTIONS).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_scalar_query_option_runs_on_spinel_with_rbs() {
    example().run_spinel_with_rbs(ASSERTIONS).assert_passes();
}

/// A Hash under a query key is Rails' nested parameters
/// (`widgets_path(filter: { a: 1 })` is `/widgets?filter%5Ba%5D=1`),
/// which the generated helper can't write. The call is left as it is
/// and the survey lists it.
#[test]
fn a_hash_query_option_is_ledgered() {
    use roundhouse::diagnostic::DiagnosticKind;
    use std::collections::HashMap;
    use std::path::PathBuf;

    let controller = "class WidgetsController < ApplicationController
  def index
    redirect_to widgets_path(filter: { kind: params[:kind] })
  end
end
";
    let mut files: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    for (path, source) in [
        ("db/schema.rb", "ActiveRecord::Schema.define do\nend\n"),
        ("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n"),
        ("app/controllers/widgets_controller.rb", controller),
        ("config/routes.rb", "Rails.application.routes.draw do\n  resources :widgets, only: :index\nend\n"),
    ] {
        files.insert(PathBuf::from(path), source.as_bytes().to_vec());
    }
    let mut app = roundhouse::ingest::ingest_app_from_tree(files).expect("ingest");
    roundhouse::session::analyze_and_lower(&mut app);
    let (emitted, diags) = roundhouse::emit::diagnostics::scope(|| roundhouse::emit::ruby::emit_lowered_controllers(&app));
    let ledgered: Vec<_> = diags
        .iter()
        .filter(|d| matches!(&d.kind, DiagnosticKind::LowerResidue { pass, construct, .. }
            if pass.as_str() == "route_query_option" && construct.as_str() == "widgets_path(filter:)"))
        .collect();
    assert_eq!(ledgered.len(), 1, "{diags:?}");
    let source = emitted
        .iter()
        .find(|f| f.path.ends_with("widgets_controller.rb"))
        .map(|f| f.content.clone())
        .expect("the emitted controller");
    assert!(source.contains("RouteHelpers.widgets_path(filter: {"), "{source}");
}

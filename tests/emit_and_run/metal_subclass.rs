//! A class under `ActionController::Metal` that mixes back the modules Rails builds Base from, as Devise's
//! `FailureApp < ActionController::Metal; include ActionController::UrlFor, ActionController::Redirecting,
//! AbstractController::Callbacks; around_action do … end`. Devise's boot names the class (`Devise::Mapping`'s
//! `@failure_app = … || Devise::FailureApp`), so it has to load; here Metal is Base under Metal's name.
//!
//! `ActionView::Rendering`, which responders asks of a controller's class (`controller.class.include?
//! ActionView::Rendering`) to pick between rendering a view and its API behaviour: Base includes it, API does not.

use super::emit_and_run;

const PROBE: &str = r#"class Probe < ActionController::Metal
  include ActionController::UrlFor
  include ActionController::Redirecting
  include AbstractController::Callbacks

  around_action do |probe, action|
    action.call
  end

  def self.ping
    "pong"
  end
end
"#;

const SCRIPT: &str = r#"raise "ping #{Probe.ping.inspect}" unless Probe.ping == "pong"
puts "metal subclass passed"
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/probe.rb", PROBE)
}

#[test]
fn a_metal_subclass_with_the_modules_base_is_built_from_loads() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("metal subclass passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_metal_subclass_with_the_modules_base_is_built_from_loads_natively() {
    let run = app().run_spinel(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("metal subclass passed"), "{}", run.stdout);
}

const API: &str = "class PingsController < ActionController::API\nend\n";

const RENDERS: &str = r#"class Renders
  def self.view?(controller)
    controller.class.include?(ActionView::Rendering)
  end
end
"#;

const BASE_SCRIPT: &str = r#"raise "base" unless Renders.view?(ActionController::Base.new)
raise "base class" unless ActionController::Base.include?(ActionView::Rendering)
"#;

fn rendering() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/renders.rb", RENDERS).write("app/controllers/pings_controller.rb", API)
}

#[test]
fn base_includes_action_view_rendering_and_api_does_not() {
    let run = rendering().run_ruby(&format!(
        "require_relative \"app/controllers/articles_controller\"\nrequire_relative \"app/controllers/pings_controller\"\n{BASE_SCRIPT}raise \"app\" unless Renders.view?(ArticlesController.new)\nraise \"api\" if Renders.view?(PingsController.new)\nraise \"api class\" if PingsController.include?(ActionView::Rendering)\nraise \"api base\" unless PingsController.include?(ActionController::Redirecting)\nputs \"view rendering passed\"\n"
    ));
    run.assert_passes();
    assert!(run.stdout.contains("view rendering passed"), "{}", run.stdout);
}

// Base only: Spinel sends `controller.class.include?` to its own Module#include?, past API's.
#[test]
#[ignore = "requires the Spinel toolchain"]
fn base_includes_action_view_rendering_natively() {
    let run = rendering().run_spinel(&format!("{BASE_SCRIPT}puts \"view rendering passed\"\n"));
    run.assert_passes();
    assert!(run.stdout.contains("view rendering passed"), "{}", run.stdout);
}

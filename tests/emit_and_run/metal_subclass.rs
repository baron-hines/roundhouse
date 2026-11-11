//! A class under `ActionController::Metal` that mixes back the modules Rails builds Base from, as Devise's
//! `FailureApp < ActionController::Metal; include ActionController::UrlFor, ActionController::Redirecting,
//! AbstractController::Callbacks; around_action do … end`. Devise's boot names the class (`Devise::Mapping`'s
//! `@failure_app = … || Devise::FailureApp`), so it has to load; here Metal is Base under Metal's name.
//!
//! `ActionView::Rendering`, which responders asks of a controller's class (`controller.class.include?
//! ActionView::Rendering`) to pick between rendering a view and its API behaviour: Base includes it, API does not.
//!
//! `logger`, which a controller reads as Rails' (Devise's `unknown_action!`: `logger.debug "…" if logger`).

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

#[test]
#[ignore = "requires the Spinel toolchain"]
fn base_includes_action_view_rendering_and_api_does_not_natively() {
    let run = rendering().run_spinel(&format!(
        "{BASE_SCRIPT}raise \"api\" if Renders.view?(PingsController.new)\nraise \"api class\" if PingsController.include?(ActionView::Rendering)\nputs \"view rendering passed\"\n"
    ));
    run.assert_passes();
    assert!(run.stdout.contains("view rendering passed"), "{}", run.stdout);
}

const LOGGER_SCRIPT: &str = r#"controller = ActionController::Base.new
raise "logger" unless controller.logger
controller.logger.debug "[probe] controller logger"
puts "controller logger passed"
"#;

#[test]
fn a_controller_logs_to_the_rails_logger() {
    let run = emit_and_run::real_blog().run_ruby(LOGGER_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("controller logger passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_controller_logs_to_the_rails_logger_natively() {
    let run = emit_and_run::real_blog().run_spinel(LOGGER_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("controller logger passed"), "{}", run.stdout);
}

/// Devise's FailureApp runs its action inside `around_action do |failure_app, action| I18n.with_locale(…, &action)
/// end`: the block wraps the dispatch `action(:respond)` reaches, with the controller as its first argument.
const AROUND: &str = r#"class Around < ActionController::Metal
  include AbstractController::Callbacks

  around_action do |around, action|
    around.trail << "before"
    action.call
    around.trail << "after"
  end

  def self.call(env)
    @respond ||= action(:respond)
    @respond.call(env)
  end

  def trail
    @trail ||= []
  end

  def respond
    trail << "respond"
  end
end
"#;

const AROUND_SCRIPT: &str = r#"around = Around.new
around.process_action(:respond)
raise "trail #{around.trail.inspect}" unless around.trail == ["before", "respond", "after"]
puts "around action passed"
"#;

#[test]
fn a_metal_actions_around_block_runs_around_it() {
    let run = emit_and_run::real_blog().write("app/models/around.rb", AROUND).run_ruby(AROUND_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("around action passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_metal_actions_around_block_runs_around_it_natively() {
    let run = emit_and_run::real_blog().write("app/models/around.rb", AROUND).run_spinel(AROUND_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("around action passed"), "{}", run.stdout);
}

/// A callback the synthesized dispatch does not run is reported, not dropped: Rails would run a `before_action`'s
/// check first, and a second `around_action` block around the first.
#[test]
fn a_metal_actions_other_callbacks_are_reported_unsupported() {
    let source = AROUND.replace(
        "  def self.call(env)",
        "  before_action :check\n\n  around_action do |around, action|\n    action.call\n  end\n\n  def self.call(env)",
    );
    let (_, errors) = emit_and_run::real_blog().write("app/models/around.rb", &source).emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("`before_action` on `Around` is not run")), "{errors:#?}");
    assert!(errors.iter().any(|e| e.contains("`around_action` on `Around` is not run")), "{errors:#?}");
}

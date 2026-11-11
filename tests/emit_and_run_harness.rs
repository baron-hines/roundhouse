//! The emit-and-run harness itself: what an overlay carries into the
//! emitted tree, the environment a run gets, and how a refused target
//! reaches the caller.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

/// Not UTF-8 (0x89 cannot start a character), so ingest carries it as a
/// binary asset rather than source, as it does a real PNG.
const PIXEL: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0xff];

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table \"widgets\" do |t|\n    t.string \"name\"\n  end\nend\n")
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n")
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
}

#[test]
fn a_run_gets_the_environment_it_is_given() {
    let run = app().run_ruby_env("print ENV.fetch(\"RH_HARNESS_PROBE\")", &[("RH_HARNESS_PROBE", "seen")]);
    run.assert_passes();
    assert_eq!(run.stdout, "seen");
}

#[test]
fn a_binary_asset_reaches_the_emitted_tree_as_the_cli_writes_it() {
    let run = app()
        .write_bytes("public/pixel.png", PIXEL)
        .run_ruby("print File.binread(\"public/pixel.png\").bytes.join(\",\")");
    run.assert_passes();
    let expected = PIXEL.iter().map(u8::to_string).collect::<Vec<_>>().join(",");
    assert_eq!(run.stdout, expected);
}

const ENGINE_LOCK: &str = "GEM\n  specs:\n    widget-engine (1.0)\n\nDEPENDENCIES\n  widget-engine\n";
const ENGINE_RBI: &str = "module WidgetEngine\n  class Client\n    sig { returns(String) }\n    def self.label; end\n  end\nend\n";

/// A run of an app whose extension refuses `widget-engine` as Rails
/// dependent, `why` being the YAML for the reason.
fn refused_run(why: &str) -> emit_and_run::Run {
    app()
        .write("roundhouse.yml", "extensions: [config/rh]\n")
        .write("config/rh/gems.yml", &format!("rails_dependent:\n  - gem: widget-engine\n    why: {why}"))
        .write("Gemfile.lock", ENGINE_LOCK)
        .write("sorbet/rbi/gems/widget-engine@1.0.rbi", ENGINE_RBI)
        .write("app/models/report.rb", "class Report\n  def self.value\n    WidgetEngine::Client.label\n  end\nend\n")
        .run_ruby("print Report.value")
}

fn assert_refused(run: &emit_and_run::Run) {
    assert!(run.errors.iter().any(|error| error.contains("widget-engine") && error.contains("requires the Rails framework")), "{:#?}", run.errors);
    assert!(!run.program_succeeded(), "a refused target has no program to run:\n{}", run.stdout);
    assert!(std::fs::read_dir(&run.emitted).unwrap().next().is_none(), "a refused target writes no tree");
}

#[test]
fn a_refused_target_reports_its_errors_instead_of_panicking() {
    assert_refused(&refused_run("requires the Rails framework\n"));
}

#[test]
fn a_refusal_whose_reason_ends_in_a_newline_is_still_reported() {
    // A block scalar keeps its trailing newline, and so does the diagnostic.
    assert_refused(&refused_run("|\n      requires the Rails framework\n"));
}

// A script reports a failed check by raising, so exit status 0 alone would
// also pass a script that stopped before its checks. The harness appends a
// completion line and requires it.

#[test]
#[should_panic(expected = "without reaching the end of its script")]
fn a_script_that_exits_before_its_checks_does_not_pass() {
    app().run_ruby("exit 0\nraise \"the check that never ran\"").assert_passes();
}

#[test]
#[should_panic(expected = "without reaching the end of its script")]
fn a_script_that_returns_before_its_checks_does_not_pass() {
    app().run_ruby("return\nraise \"the check that never ran\"").assert_passes();
}

#[test]
fn a_script_that_stops_early_is_not_a_success_either() {
    let run = app().run_ruby("exit 0");
    assert!(!run.program_succeeded(), "a caller reading `program_succeeded` gets the same verdict as `assert_passes`");
}

#[test]
fn output_after_the_script_ends_is_kept_and_the_completion_line_is_not() {
    let run = app().run_ruby("at_exit { print \"after\" }\nputs \"during\"");
    run.assert_passes();
    assert_eq!(run.stdout, "during\nafter");
}

// A program that runs is half the claim: a run whose analysis reports an
// error has not passed, however its program ran. `passed` and
// `assert_passes` read both halves; `program_succeeded` reads one.

const PUNDIT_LOCK: &str = "GEM\n  remote: https://rubygems.org/\n  specs:\n    pundit (2.5.2)\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n  pundit\n";

/// An action no script calls refers to an unshipped gem's helper: `check`
/// refuses it, and the emitted program, which never reaches it, runs.
fn refused_but_running() -> emit_and_run::Run {
    app()
        .write("Gemfile.lock", PUNDIT_LOCK)
        .write("app/models/widget.rb", "class Widget < ApplicationRecord\nend\n")
        .write("app/controllers/widgets_controller.rb", "class WidgetsController < ApplicationController\n  def index\n    @widgets = policy_scope(Widget)\n  end\nend\n")
        .run_ruby("print :ran")
}

#[test]
fn a_run_that_reports_an_error_has_not_passed_although_its_program_ran() {
    let run = refused_but_running();
    assert!(run.errors.iter().any(|error| error.contains("`policy_scope` is a helper of the `pundit` gem")), "{:#?}", run.errors);
    assert!(run.program_succeeded(), "{}\n{}", run.stdout, run.stderr);
    assert_eq!(run.stdout, "ran");
    assert!(!run.passed(), "a run with an error passed");
}

#[test]
#[should_panic(expected = "analysis or emit reports errors")]
fn assert_passes_fails_a_run_that_reports_an_error() {
    refused_but_running().assert_passes();
}

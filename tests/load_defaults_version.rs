//! Installed Rails versions must take the same JSON-default path as literals.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;
#[path = "support/json_response_escaping.rs"]
mod json_response_escaping;
#[path = "support/scratch.rs"]
mod scratch;

use roundhouse::{diagnostic::Severity, ingest, vfs::{FileTree, MapVfs}};
use std::{path::{Path, PathBuf}, process::Command};

const LOBSTERS: &str = r##""#{Rails::VERSION::MAJOR}.#{Rails::VERSION::MINOR}""##;

fn application(expression: &str) -> String {
    format!("module Lobsters\n  class Application < Rails::Application\n    config.load_defaults {expression}\n  end\nend\n")
}

fn lock(gem: &str, version: &str) -> String {
    format!("GEM\n  remote: https://rubygems.org/\n  specs:\n    {gem} ({version})\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n  {gem}\n")
}

fn files(expression: &str, gem: &str, version: &str) -> FileTree {
    [("config/application.rb", application(expression)),
     ("Gemfile.lock", lock(gem, version)),
     ("db/schema.rb", "ActiveRecord::Schema.define do\nend\n".into()),
     ("config/routes.rb", "Rails.application.routes.draw do\nend\n".into())]
        .into_iter().map(|(p, s)| (PathBuf::from("/app").join(p), s.into_bytes())).collect()
}

#[test]
fn lobsters_interpolation_ingests_with_the_locked_rails_version() {
    let vfs = MapVfs::new(files(LOBSTERS, "rails", "8.1.4"));
    let (app, diagnostics) = ingest::prism::scope(|| ingest::ingest_app_with_vfs(&vfs, Path::new("/app")));
    app.expect("ingest Lobsters' load_defaults");
    assert!(!diagnostics.iter().any(|d| d.code() == "unsupported" && d.message.contains("load_defaults")), "{diagnostics:?}");
}

#[test]
fn installed_version_to_f_ingests_with_railties_only() {
    let vfs = MapVfs::new(files("Rails::VERSION::STRING.to_f", "railties", "8.1.4"));
    ingest::ingest_app_with_vfs(&vfs, Path::new("/app")).expect("ingest installed Rails STRING.to_f");
}

#[test]
fn installed_defaults_match_literal_json_response_bytes() {
    for (gem, version, defaults) in [("rails", "8.1.4", "8.1"), ("railties", "8.0.2", "8.0")] {
        for expression in [LOBSTERS, "Rails::VERSION::STRING.to_f", "::Rails::VERSION::STRING.to_f", "Rails.version.to_f"] {
            json_response_escaping::app(expression).write("Gemfile.lock", &lock(gem, version))
                .run_ruby(&json_response_escaping::program(defaults)).assert_passes();
        }
    }
}

#[test]
fn literal_defaults_keep_their_version_even_with_newer_installed_rails() {
    json_response_escaping::app("8.0").write("Gemfile.lock", &lock("rails", "8.1.4"))
        .run_ruby(&json_response_escaping::program("8.0")).assert_passes();
}

#[test]
fn unresolved_defaults_ingest_and_report_one_source_located_unsupported_setting() {
    for expression in ["ENV.fetch('DEFAULTS')", "UNKNOWN", "Rails::VERSION::STRING.to_f + 1"] {
        let vfs = MapVfs::new(files(expression, "rails", "8.1.4"));
        let (app, diagnostics) = ingest::prism::scope(|| ingest::ingest_app_with_vfs(&vfs, Path::new("/app")));
        let app = app.expect("unknown defaults must not abort ingestion");
        let diagnostics: Vec<_> = diagnostics.iter().filter(|d| d.code() == "unsupported" && d.message.contains("load_defaults")).collect();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let diagnostic = diagnostics[0];
        assert_eq!(diagnostic.code(), "unsupported");
        assert_eq!(diagnostic.severity, Severity::Error);
        let source = &app.sources[diagnostic.span.file.0 as usize - 1];
        assert!(source.path.ends_with("config/application.rb"));
        assert_eq!(source.line_col(diagnostic.span.start), (3, 5));
    }
}

#[test]
fn unresolved_defaults_are_an_error_in_strict_emit_and_a_warning_otherwise() {
    let root = scratch::dir(&format!("roundhouse_defaults_{}", std::process::id()));
    for (path, content) in files("ENV.fetch('DEFAULTS')", "rails", "8.1.4") {
        let path = root.join(path.strip_prefix("/app").unwrap());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    for permissive in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_roundhouse"));
        command.args(["--target", "ruby"]);
        if permissive { command.arg("--allow-unsupported"); }
        let output = command.arg(&root).arg("-o").arg(root.join(if permissive { "allow" } else { "strict" }))
            .output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.success(), permissive, "{stderr}");
        let level = if permissive { "warning[unsupported]" } else { "error[unsupported]" };
        assert!(stderr.lines().any(|line| line.contains(level) && line.contains("load_defaults")), "{stderr}");
        assert!(stderr.contains("config/application.rb:3:5"), "{stderr}");
    }
    let output = emit_and_run::ruby().current_dir(root.join("allow"))
        .args(["-I.", "-e", "require './main'; a = Rails.application; raise 'controller fallback' unless a.action_controller_escape_json_responses; raise 'separator fallback' unless a.active_support_escape_js_separators_in_json; raise 'HTML fallback' unless a.active_support_escape_html_entities_in_json"])
        .output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_lock_and_other_version_expressions_remain_unresolved() {
    for expression in [LOBSTERS, "Other::VERSION::STRING.to_f", "Rails::VERSION::STRING.to_f + 1",
        r##""#{Rails::VERSION::MAJOR} .#{Rails::VERSION::MINOR}""##,
        r##"'#{Rails::VERSION::MAJOR}.#{Rails::VERSION::MINOR}'"##] {
        let mut tree = files(expression, "rails", "8.1.4");
        if expression == LOBSTERS { tree.remove(&PathBuf::from("/app/Gemfile.lock")); }
        let (app, diagnostics) = ingest::prism::scope(|| ingest::ingest_app_with_vfs(&MapVfs::new(tree), Path::new("/app")));
        app.expect("unknown defaults ingest");
        assert_eq!(diagnostics.iter().filter(|d| d.code() == "unsupported" && d.message.contains("load_defaults")).count(), 1,
            "{expression}: {diagnostics:?}");
    }
}

#[test]
fn explicit_json_assignments_shadow_unresolved_defaults_per_setting() {
    for separators in [false, true] {
        let mut tree = files("ENV.fetch('DEFAULTS')", "rails", "8.1.4");
        let assignments = if separators { "config.active_support.escape_js_separators_in_json = false\n" } else { "" };
        tree.insert(PathBuf::from("/app/config/application.rb"),
            format!("module Lobsters\nclass Application < Rails::Application\nconfig.load_defaults ENV.fetch('DEFAULTS')\nconfig.action_controller.escape_json_responses = false\n{assignments}end\nend\n").into_bytes());
        let (app, diagnostics) = ingest::prism::scope(|| ingest::ingest_app_with_vfs(&MapVfs::new(tree), Path::new("/app")));
        app.expect("overridden defaults ingest");
        assert_eq!(diagnostics.iter().filter(|d| d.code() == "unsupported" && d.message.contains("load_defaults")).count(), usize::from(!separators), "{diagnostics:?}");
    }
}

#[test]
#[ignore = "requires a ruby-bench checkout in LOBSTERS_APP"]
fn ruby_bench_lobsters_ingests() {
    let path = std::env::var_os("LOBSTERS_APP").expect("set LOBSTERS_APP to ruby-bench/benchmarks/lobsters");
    ingest::ingest_app(Path::new(&path)).expect("ingest the real ruby-bench Lobsters application");
}

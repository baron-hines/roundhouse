//! `Rails.root.join("tmp", "x").expand_path`: a Pathname in Rails, and
//! absolute, as the root is.
//!
//! `Rails.root` answers runtime/ruby/rails.rb's `Rails::AppPath`, which
//! had no `expand_path`, so the call had no type and every read of a
//! constant built from it was refused as `constant not supported`; the
//! emitted program would have raised NoMethodError where the class loads.
//! The emitted app's root is the directory it serves from, so CRuby's
//! Pathname over that directory is the oracle (Rails 8.1.4's `Rails.root`
//! is that Pathname): an absolute part starts a join over, a part's
//! trailing separator is not doubled, and a directory argument is
//! converted, though it never moves an absolute path (`Rails.public_path`
//! is absolute in Rails, so `expand_path("/srv")` keeps it). The
//! targets without the ruby-family runtime refuse the call by name,
//! whichever arm of its receiver is the app path.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;

const PATHS: &str = r#"module Paths
  CACHE = Rails.root.join("tmp", "cache").expand_path
  LOGS = Rails.root.join("tmp", "..", "log").expand_path
  UNDER = Rails.root.join("x").expand_path("/srv")
  VAR = Rails.root.join("/var", "x").expand_path
  RESTART = Rails.root.join("tmp", "/var").expand_path
  PLUS = (Rails.root + "/var").expand_path
  TRAILING = Rails.root.join("/var/", "x")
  SLASHES = Rails.root.join("/var//", "x")
  ROOTED = Rails.root.join("/", "x")
  PUBLIC = Rails.public_path.expand_path("/srv")

  def self.report
    [CACHE.to_s, LOGS.to_s, UNDER.to_s, CACHE.join("a").to_s, VAR.to_s, RESTART.to_s, PLUS.to_s,
     TRAILING.to_s, SLASHES.to_s, ROOTED.to_s, PUBLIC.to_s]
  end

  def self.bad_dir
    Rails.root.join("x").expand_path(12).to_s
  rescue TypeError => e
    e.class.name
  end
end
"#;

#[test]
fn an_expanded_app_path_is_typed_and_runs_like_pathname() {
    let app = || emit_and_run::empty_app().write("app/lib/paths.rb", PATHS);
    let errors = app().check_errors();
    assert!(errors.is_empty(), "check refused an expanded Rails.root path:\n{}", errors.join("\n"));

    // The emitted app's root is the working directory `run_ruby` boots in.
    let run = app().run_ruby(
        r#"require "pathname"
root = Pathname(Dir.pwd)
expected = [root.join("tmp", "cache").expand_path.to_s, root.join("tmp", "..", "log").expand_path.to_s,
            root.join("x").expand_path("/srv").to_s, root.join("tmp", "cache", "a").expand_path.to_s,
            root.join("/var", "x").expand_path.to_s, root.join("tmp", "/var").expand_path.to_s,
            (root + "/var").expand_path.to_s, root.join("/var/", "x").to_s, root.join("/var//", "x").to_s,
            root.join("/", "x").to_s, root.join("public").expand_path("/srv").to_s]
bad = begin
  root.join("x").expand_path(12).to_s
rescue TypeError => e
  e.class.name
end
p Paths.report == expected || [Paths.report, expected], Paths.bad_dir == bad, Paths.bad_dir,
  Paths.report.map { |path| Pathname(path).absolute? }.uniq
"#,
    );
    run.assert_passes();
    assert_eq!(run.stdout, "true\ntrue\n\"TypeError\"\n[true]\n");

    let (_, errors) = app().emit(BuildTarget::Typescript);
    assert!(
        errors.iter().any(|e| e.contains("Rails::AppPath#expand_path not supported (typescript)")),
        "typescript must refuse Rails::AppPath#expand_path: {errors:?}"
    );
}

/// The targets without the ruby-family runtime.
const STRICT: [BuildTarget; 10] = [
    BuildTarget::Crystal, BuildTarget::CSharp, BuildTarget::Elixir, BuildTarget::Go, BuildTarget::Kotlin,
    BuildTarget::Python, BuildTarget::Rust, BuildTarget::Swift, BuildTarget::Typescript, BuildTarget::TypescriptWorker,
];

/// A receiver that may be the app path, nullable or in a union, is refused
/// on every strict target, as a receiver that is only the app path is.
#[test]
fn every_strict_target_refuses_expand_path_on_any_app_path_arm() {
    let schema = "ActiveRecord::Schema.define do\n  create_table \"probes\" do |t|\n    t.string \"name\"\n  end\nend\n";
    let record = "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n";
    let probe = "class Probe < ApplicationRecord\n  def self.nullable(value) = value.expand_path.to_s\n  \
                 def self.with_string(value) = value.expand_path.to_s\n  \
                 def self.with_pathname(value) = value.expand_path.to_s\nend\n";
    let sig = "class Probe < ApplicationRecord\n  def self.nullable: (Rails::AppPath?) -> String\n  \
               def self.with_string: (Rails::AppPath | String) -> String\n  \
               def self.with_pathname: (Rails::AppPath | Pathname) -> String\nend\n";
    for target in STRICT {
        let (_, errors) = emit_and_run::empty_app()
            .write("db/schema.rb", schema)
            .write("app/models/application_record.rb", record)
            .write("app/models/probe.rb", probe)
            .write("sig/probe.rbs", sig)
            .emit(target);
        let wanted = format!("Rails::AppPath#expand_path not supported ({})", target.as_str());
        assert_eq!(errors.iter().filter(|e| e.contains(&wanted)).count(), 3, "{}: {errors:?}", target.as_str());
    }
}

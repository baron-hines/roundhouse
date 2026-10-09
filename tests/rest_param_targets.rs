//! A positional rest (`def f(*args)`, and the anonymous `def f(*)`)
//! is a ruby-family construct. Every other emitter rendered it as ONE
//! required parameter — `named(args: any)`, `func named(_ args: Any?)`
//! — so `f()` and `f(a, b)` no longer matched the declaration and
//! `f(a)` handed the body `a` where Ruby hands it `[a]`, while `check`
//! reported nothing. Those targets now say so; the ruby family, which
//! renders `*args`, does not.
//!
//! A controller method has no rest slot at all (`Action` records
//! required, optional and keyword parameters), so `def pick(*keys)`
//! was emitted as `def pick` on EVERY target. It is refused at ingest
//! the way the other unretained controller formals are, and a concern
//! method spliced into a controller is ledgered on its own def.

use std::collections::HashMap;
use std::path::PathBuf;

use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::project::BuildTarget;

const SCHEMA: &str = "ActiveRecord::Schema.define do\n  create_table \"gauges\", force: :cascade do |t|\n    t.string \"label\", null: false\n  end\nend\n";

fn tree(extra: &[(&str, &str)]) -> HashMap<PathBuf, Vec<u8>> {
    let mut files: Vec<(&str, &str)> = vec![
        ("db/schema.rb", SCHEMA),
        ("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\nend\n"),
        (
            "app/controllers/application_controller.rb",
            "class ApplicationController < ActionController::Base\nend\n",
        ),
        (
            "config/routes.rb",
            "Rails.application.routes.draw do\n  get \"/gauges\", to: \"gauges#index\"\nend\n",
        ),
    ];
    files.extend_from_slice(extra);
    files
        .into_iter()
        .map(|(p, c)| (PathBuf::from(p), c.as_bytes().to_vec()))
        .collect()
}

const MODEL: &str = "class Gauge < ApplicationRecord
  def named(*args)
    \"named\"
  end

  def anon(*)
    \"anon\"
  end

  def plain(a, b = 1)
    \"plain\"
  end
end
";

const HELPER: &str = "module GaugesHelper
  def mark(*)
    \"[mark]\"
  end
end
";

fn ledgered(target: BuildTarget) -> Vec<String> {
    let mut app = ingest_app_from_tree(tree(&[
        ("app/models/gauge.rb", MODEL),
        ("app/helpers/gauges_helper.rb", HELPER),
    ]))
    .expect("ingest");
    roundhouse::session::analyze_and_lower(&mut app);
    let (_, diags) = roundhouse::emit::diagnostics::scope(|| {
        let _ = roundhouse::project::target_files(&app, std::path::Path::new("."), target);
    });
    diags
        .iter()
        .map(roundhouse::diagnostic::Diagnostic::to_string)
        .filter(|d| d.contains("rest parameter"))
        .collect()
}

#[test]
fn a_target_without_a_rest_carrier_reports_each_rest_parameter() {
    for target in [
        BuildTarget::Typescript,
        BuildTarget::Swift,
        BuildTarget::Go,
        BuildTarget::Python,
        BuildTarget::Rust,
        BuildTarget::Kotlin,
        BuildTarget::CSharp,
        BuildTarget::Crystal,
        BuildTarget::Elixir,
    ] {
        let entries = ledgered(target);
        assert_eq!(entries.len(), 3, "{target:?}: one per rest parameter; got {entries:?}");
        assert!(entries.iter().any(|d| d.contains("`*args` on `named`")), "{target:?}: {entries:?}");
        assert!(entries.iter().any(|d| d.contains("`*` on `anon`")), "{target:?}: {entries:?}");
        assert!(entries.iter().any(|d| d.contains("`*` on `mark`")), "{target:?}: {entries:?}");
        assert!(!entries.iter().any(|d| d.contains("`plain`")), "{target:?}: {entries:?}");
    }
}

#[test]
fn the_ruby_family_does_not_report_what_it_renders() {
    for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
        let entries = ledgered(target);
        assert!(entries.is_empty(), "{target:?} renders `*args`; got {entries:?}");
    }
}

#[test]
fn a_controller_rest_parameter_is_refused_rather_than_dropped() {
    for def in ["def pick(*keys)", "def pick(*)", "def pick(first, *rest, last)"] {
        let controller = format!(
            "class GaugesController < ApplicationController\n  def index\n    @out = pick(:a, :b, :c)\n  end\n\n  private\n\n  {def}\n    \"picked\"\n  end\nend\n"
        );
        let err = ingest_app_from_tree(tree(&[("app/controllers/gauges_controller.rb", &controller)]))
            .err()
            .unwrap_or_else(|| panic!("`{def}` must not ingest as `def pick`"));
        assert!(
            err.to_string().contains("positional rest declaration on a controller method"),
            "`{def}`: {err}"
        );
    }
}

#[test]
fn a_concern_rest_parameter_spliced_into_a_controller_is_ledgered() {
    let mut app = ingest_app_from_tree(tree(&[
        (
            "app/controllers/concerns/picker.rb",
            "module Picker\n  extend ActiveSupport::Concern\n\n  private\n    def pick(*keys)\n      \"picked\"\n    end\nend\n",
        ),
        (
            "app/controllers/gauges_controller.rb",
            "class GaugesController < ApplicationController\n  include Picker\n\n  def index\n    @out = pick(:a, :b)\n  end\nend\n",
        ),
    ]))
    .expect("ingest");
    // The `check` path: analyze without the post-analyze lowerings.
    let mut analyzer = roundhouse::analyze::Analyzer::new(&app);
    analyzer.analyze(&mut app);
    let diags = roundhouse::analyze::diagnose(&app);
    let found: Vec<String> = diags
        .iter()
        .map(roundhouse::diagnostic::Diagnostic::to_string)
        .filter(|d| d.contains("spliced into a controller"))
        .collect();
    assert_eq!(found.len(), 1, "got {found:?}");
}

//! Database identity: `config/database.yml` resolved at build time, and
//! which database each model runs on, read from source.
//!
//! The configurations are Rails' (`ActiveRecord::DatabaseConfigurations`),
//! and model ownership follows `connects_to` through abstract parents.
//! The emitted runtime opens only the primary, so every routing it does
//! not carry is an error at its span, and a `connects_to` that resolves
//! to the primary checks clean and runs.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use std::collections::HashMap;
use std::path::PathBuf;

use roundhouse::analyze::{diagnose, Analyzer};
use roundhouse::diagnostic::Severity;
use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::App;

const DATABASE_YML: &str = "test:\n  primary:\n    adapter: sqlite3\n    database: tmp/test.sqlite3\n  archive:\n    adapter: sqlite3\n    database: tmp/archive.sqlite3\n    migrations_paths: db/archive_migrate\n";
const SCHEMA: &str = "ActiveRecord::Schema.define do\n  create_table \"posts\", force: :cascade do |t|\n    t.string \"title\"\n  end\nend\n";
const APP_RECORD: &str = "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n";
const POST: &str = "class Post < ApplicationRecord\nend\n";
const APP_CONTROLLER: &str = "class ApplicationController < ActionController::Base\nend\n";
const ROUTES: &str = "Rails.application.routes.draw do\nend\n";
const ARCHIVE_RECORD: &str = "class ArchiveRecord < ApplicationRecord\n  self.abstract_class = true\n  connects_to database: { writing: :archive }\nend\n";
const COLD_RECORD: &str = "class ColdRecord < ArchiveRecord\n  self.abstract_class = true\nend\n";

fn base_files() -> Vec<(String, String)> {
    [
        ("app/models/application_record.rb", APP_RECORD),
        ("app/models/post.rb", POST),
        ("app/controllers/application_controller.rb", APP_CONTROLLER),
        ("config/routes.rb", ROUTES),
        ("config/database.yml", DATABASE_YML),
        ("db/schema.rb", SCHEMA),
    ]
    .into_iter()
    .map(|(path, content)| (path.to_string(), content.to_string()))
    .collect()
}

fn put(files: &mut Vec<(String, String)>, path: &str, content: &str) {
    files.retain(|(existing, _)| existing != path);
    files.push((path.to_string(), content.to_string()));
}

struct Checked {
    app: App,
    /// Every error diagnostic, rendered `path:line:col: error[code]: message`.
    errors: Vec<String>,
    warnings: Vec<String>,
}

fn check(files: &[(String, String)]) -> Checked {
    let tree: HashMap<PathBuf, Vec<u8>> =
        files.iter().map(|(path, content)| (PathBuf::from(path), content.as_bytes().to_vec())).collect();
    let mut app = ingest_app_from_tree(tree).unwrap_or_else(|error| panic!("ingest failed: {error}"));
    Analyzer::new(&app).analyze(&mut app);
    let diagnostics = diagnose(&app);
    let rendered = |severity: Severity| {
        diagnostics.iter().filter(|d| d.severity == severity).map(|d| d.render(&app.sources)).collect::<Vec<_>>()
    };
    let (errors, warnings) = (rendered(Severity::Error), rendered(Severity::Warning));
    Checked { app, errors, warnings }
}

/// The diagnostic at `location` (a `path:line:` prefix) whose text has
/// every one of `phrases`; panics with all of them when there is none.
fn assert_at(diagnostics: &[String], location: &str, phrases: &[&str]) {
    let hit = diagnostics.iter().any(|d| d.starts_with(location) && phrases.iter().all(|p| d.contains(p)));
    assert!(hit, "expected a diagnostic at {location} naming {phrases:?}; got:\n{}", diagnostics.join("\n"));
}

#[test]
fn the_configurations_are_read_at_build_time() {
    let checked = check(&base_files());
    assert!(checked.errors.is_empty(), "{}", checked.errors.join("\n"));
    let databases = serde_json::to_value(&checked.app).unwrap()["databases"].clone();
    assert_eq!(databases[0]["env"], "test", "{databases}");
    assert_eq!(databases[0]["name"], "primary", "{databases}");
    assert_eq!(databases[0]["adapter"], "sqlite3", "{databases}");
    assert_eq!(databases[0]["primary"], true, "{databases}");
    assert_eq!(databases[1]["name"], "archive", "{databases}");
    assert_eq!(databases[1]["primary"], false, "{databases}");
    assert_eq!(databases[1]["migrations_paths"][0], "db/archive_migrate", "{databases}");
    // Nothing routes away from the primary, so no ownership is recorded.
    let ir = serde_json::to_value(&checked.app).unwrap();
    for key in ["model_databases", "database_refusals"] {
        assert!(ir.get(key).is_none(), "an app with no routing serializes `{key}`: {}", ir[key]);
    }
}

#[test]
fn an_app_without_database_yml_serializes_no_database_fields() {
    let mut files = base_files();
    files.retain(|(path, _)| path != "config/database.yml");
    let ir = serde_json::to_value(&check(&files).app).unwrap();
    for key in ["databases", "model_databases", "database_refusals"] {
        assert!(ir.get(key).is_none(), "an app without config/database.yml serializes `{key}`: {}", ir[key]);
    }
}

#[test]
fn a_computed_adapter_is_a_warning_at_its_configuration_not_an_error() {
    let mut files = base_files();
    put(
        &mut files,
        "config/database.yml",
        "test:\n  primary:\n    adapter: sqlite3\n    database: <%= ENV.fetch(\"TEST_DB\") { \"tmp/test.sqlite3\" } %>\n  archive:\n    adapter: <%= ENV[\"ARCHIVE_ADAPTER\"] %>\n",
    );
    let checked = check(&files);
    assert!(checked.errors.is_empty(), "{}", checked.errors.join("\n"));
    assert_at(&checked.warnings, "config/database.yml:5:", &["`test.archive`", "`adapter`", "ERB"]);
    let databases = serde_json::to_value(&checked.app).unwrap()["databases"].clone();
    assert_eq!(databases.as_array().map(Vec::len), Some(1), "{databases}");
}

fn two_database_files() -> Vec<(String, String)> {
    let mut files = base_files();
    put(&mut files, "app/models/archive_record.rb", ARCHIVE_RECORD);
    put(&mut files, "app/models/archived_post.rb", "class ArchivedPost < ArchiveRecord\nend\n");
    put(&mut files, "app/models/cold_record.rb", COLD_RECORD);
    put(&mut files, "app/models/cold_post.rb", "class ColdPost < ColdRecord\nend\n");
    files
}

#[test]
fn ownership_follows_connects_to_through_abstract_parents() {
    let checked = check(&two_database_files());
    let owners = serde_json::to_value(&checked.app).unwrap()["model_databases"].clone();
    for model in ["ArchiveRecord", "ArchivedPost", "ColdRecord", "ColdPost"] {
        assert_eq!(owners[model]["named"]["writing"], "archive", "{model}: {owners}");
        assert_eq!(owners[model]["named"]["declared_by"], "ArchiveRecord", "{model}: {owners}");
    }
    assert_eq!(owners["Post"], "primary", "{owners}");
    assert_eq!(owners["ApplicationRecord"], "primary", "{owners}");
    // One error, at the declaration, counting the models it owns; an
    // abstract class asks for no table of its own.
    let archive: Vec<_> = checked.errors.iter().filter(|e| e.contains("`archive`")).collect();
    assert_eq!(archive.len(), 1, "{}", checked.errors.join("\n"));
    assert_at(&checked.errors, "app/models/archive_record.rb:3:", &["`archive`", "sqlite3", "only the primary", "4 model"]);
    assert!(
        !checked.errors.iter().any(|e| e.contains("archive_records") || e.contains("cold_records")),
        "{}",
        checked.errors.join("\n")
    );
}

#[test]
fn a_second_database_names_its_adapter() {
    for adapter in ["postgresql", "trilogy"] {
        let mut files = two_database_files();
        put(&mut files, "config/database.yml", &DATABASE_YML.replacen("adapter: sqlite3\n    database: tmp/archive", &format!("adapter: {adapter}\n    database: tmp/archive"), 1));
        let checked = check(&files);
        assert_at(&checked.errors, "app/models/archive_record.rb:3:", &["`archive`", &format!("({adapter})"), "only the primary"]);
    }
}

#[test]
fn each_routing_shape_the_runtime_does_not_carry_is_refused_at_its_span() {
    let cases: Vec<(&str, &str, &str, &[&str])> = vec![
        (
            "app/models/archive_record.rb",
            "class ArchiveRecord < ApplicationRecord\n  self.abstract_class = true\n  connects_to database: DATABASES\nend\n",
            "app/models/archive_record.rb:3:",
            &["connects_to", "literal"],
        ),
        (
            "app/models/archive_record.rb",
            "class ArchiveRecord < ApplicationRecord\n  self.abstract_class = true\n  if ENV[\"ARCHIVE\"]\n    connects_to database: { writing: :archive }\n  end\nend\n",
            "app/models/archive_record.rb:4:",
            &["connects_to", "condition"],
        ),
        (
            "app/models/archive_record.rb",
            "class ArchiveRecord < ApplicationRecord\n  self.abstract_class = true\n  connects_to shards: { one: { writing: :archive } }\nend\n",
            "app/models/archive_record.rb:3:",
            &["connects_to", "shards"],
        ),
        (
            "app/models/archive_record.rb",
            "class ArchiveRecord < ApplicationRecord\n  connects_to database: { writing: :archive }\nend\n",
            "app/models/archive_record.rb:2:",
            &["connects_to", "abstract"],
        ),
        (
            "app/models/archive_record.rb",
            "class ArchiveRecord < ApplicationRecord\n  self.abstract_class = true\n  connects_to database: { writing: :nowhere }\nend\n",
            "app/models/archive_record.rb:3:",
            &["`nowhere`", "does not configure"],
        ),
        (
            "app/models/post.rb",
            "class Post < ApplicationRecord\n  def self.archived\n    ApplicationRecord.connected_to(role: :reading) { 1 }\n  end\nend\n",
            "app/models/post.rb:3:",
            &["connected_to", "role and shard switching is not modeled"],
        ),
        (
            "app/models/post.rb",
            "class Post < ApplicationRecord\n  def self.connection_pool\n    super\n  end\nend\n",
            "app/models/post.rb:2:",
            &["connection_pool", "app-defined"],
        ),
        (
            "app/models/post.rb",
            "class Post < ApplicationRecord\n  establish_connection :archive\nend\n",
            "app/models/post.rb:2:",
            &["establish_connection"],
        ),
    ];
    let mut missed = Vec::new();
    for (path, source, location, phrases) in cases {
        let mut files = base_files();
        put(&mut files, path, source);
        let checked = check(&files);
        if !checked.errors.iter().any(|e| e.starts_with(location) && phrases.iter().all(|p| e.contains(p))) {
            missed.push(format!("{source:?}: expected {location} {phrases:?}, got {:?}", checked.errors));
        }
    }
    assert!(missed.is_empty(), "{} not refused at their span:\n{}", missed.len(), missed.join("\n"));
}

/// Only Active Record's own switches are refused: a call on another
/// object, or one without the `role:` / `shard:` that Rails requires of
/// `connected_to`, is the app's own method of the same name.
#[test]
fn a_method_named_like_a_switch_on_another_object_is_not_refused() {
    let mut files = base_files();
    put(
        &mut files,
        "app/models/station.rb",
        "class Station\n  def initialize(links)\n    @links = links\n  end\n\n  def reachable(other)\n    @links.connected_to(other)\n  end\n\n  def handler(registry)\n    registry.connection_handler\n  end\n\n  def connected_to(other)\n    @links.include?(other)\n  end\n\n  def linked?(other)\n    connected_to(other)\n  end\nend\n",
    );
    let checked = check(&files);
    let switches: Vec<&String> = checked.errors.iter().filter(|e| e.contains("connected_to") || e.contains("connection_handler")).collect();
    assert!(switches.is_empty(), "{switches:#?}");
}

/// A switch in a controller's class body (an `around_action` block) runs
/// every request on another role, so it is refused there too.
#[test]
fn a_switch_in_a_controller_filter_block_is_refused() {
    let mut files = base_files();
    put(
        &mut files,
        "app/controllers/reports_controller.rb",
        "class ReportsController < ApplicationController\n  around_action do |_controller, action|\n    ActiveRecord::Base.connected_to(role: :reading) { action.call }\n  end\n\n  def index\n    head :ok\n  end\nend\n",
    );
    let checked = check(&files);
    assert_at(&checked.errors, "app/controllers/reports_controller.rb:3:", &["connected_to", "role and shard switching is not modeled"]);
}

/// A concern's `class_methods` override reaches every includer; the error
/// is at the concern's `def`, and the includer's ownership says so.
#[test]
fn a_connection_override_from_a_concern_is_refused_at_its_def() {
    let mut files = base_files();
    put(
        &mut files,
        "app/models/concerns/routed.rb",
        "module Routed\n  extend ActiveSupport::Concern\n\n  class_methods do\n    def connection_pool\n      super\n    end\n  end\nend\n",
    );
    put(&mut files, "app/models/post.rb", "class Post < ApplicationRecord\n  include Routed\nend\n");
    let checked = check(&files);
    assert_at(&checked.errors, "app/models/concerns/routed.rb:5:", &["connection_pool", "app-defined", "Post"]);
    let owners = serde_json::to_value(&checked.app).unwrap()["model_databases"].clone();
    assert_eq!(owners["Post"]["app_defined"]["method"], "connection_pool", "{owners}");
    assert_eq!(owners["Post"]["app_defined"]["defined_by"], "Routed", "{owners}");
}

/// A concern that defines several lookups (`with_connection`,
/// `lease_connection`, `connection_pool`) is one app-defined routing, as
/// Rails consults all of them: one error at the first def, counting every
/// includer, and no "declares its connection more than once".
#[test]
fn several_lookups_from_one_concern_are_one_app_defined_routing() {
    let mut files = base_files();
    put(
        &mut files,
        "app/models/concerns/sharded_table.rb",
        "module ShardedTable\n  extend ActiveSupport::Concern\n\n  class_methods do\n    def with_connection(&block)\n      connection_pool.with_connection(&block)\n    end\n\n    def lease_connection\n      super\n    end\n    alias_method :connection, :lease_connection\n\n    def connection_pool\n      super\n    end\n  end\nend\n",
    );
    put(&mut files, "app/models/post.rb", "class Post < ApplicationRecord\n  include ShardedTable\nend\n");
    put(&mut files, "app/models/comment.rb", "class Comment < ApplicationRecord\n  include ShardedTable\nend\n");
    let checked = check(&files);
    let routing: Vec<&String> = checked.errors.iter().filter(|e| e.contains("app-defined") || e.contains("more than once")).collect();
    assert_eq!(routing.len(), 1, "{routing:#?}");
    assert_at(
        &checked.errors,
        "app/models/concerns/sharded_table.rb:5:",
        &["with_connection", "app-defined", "ShardedTable", "2 model(s)", "Post", "Comment"],
    );
}

/// `connects_to` without `config/database.yml` has nothing to resolve its
/// names against, so it is refused rather than run on the primary.
#[test]
fn connects_to_without_database_yml_is_refused() {
    let mut files = base_files();
    files.retain(|(path, _)| path != "config/database.yml");
    put(&mut files, "app/models/archive_record.rb", ARCHIVE_RECORD);
    let checked = check(&files);
    assert_at(&checked.errors, "app/models/archive_record.rb:3:", &["connects_to", "no config/database.yml"]);
}

/// A class-body `connects_to` / `establish_connection` is decided by the
/// database ownership path, so model lowering's dropped-DSL check does
/// not also report it: a primary-owned `connects_to` checks clean, and
/// each routing shape the runtime does not carry is refused once.
#[test]
fn routing_statements_are_decided_by_the_database_path_not_reported_as_dropped_dsl() {
    let mut files = base_files();
    put(
        &mut files,
        "app/models/application_record.rb",
        "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\n  connects_to database: { writing: :primary }\nend\n",
    );
    let checked = check(&files);
    assert!(checked.errors.is_empty(), "{}", checked.errors.join("\n"));
    assert!(!checked.warnings.iter().any(|w| w.contains("connects_to")), "{}", checked.warnings.join("\n"));

    for (path, source, location) in [
        ("app/models/post.rb", "class Post < ApplicationRecord\n  establish_connection :archive\nend\n", "app/models/post.rb:2:"),
        (
            "app/models/archive_record.rb",
            "class ArchiveRecord < ApplicationRecord\n  connects_to database: { writing: :archive }\nend\n",
            "app/models/archive_record.rb:2:",
        ),
    ] {
        let mut files = base_files();
        put(&mut files, path, source);
        let checked = check(&files);
        let at: Vec<&String> = checked.errors.iter().filter(|e| e.starts_with(location)).collect();
        assert_eq!(at.len(), 1, "{source:?}: expected one database error at {location}; errors:\n{}", checked.errors.join("\n"));
    }
}

/// A `connects_to` whose writing role is the primary is not refused,
/// which is a claim that the emitted program runs with it.
#[test]
fn connects_to_the_primary_checks_clean_and_the_emitted_program_runs() {
    let mut overlay = emit_and_run::empty_app();
    for (path, content) in base_files() {
        overlay = overlay.write(&path, &content);
    }
    overlay
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\n  connects_to database: { writing: :primary }\nend\n",
        )
        .run_ruby(
            r#"
Post.create!(title: "kept")
raise "count #{Post.count}" unless Post.count == 1
raise "title" unless Post.first.title == "kept"
puts "primary-owned connects_to ran"
"#,
        )
        .assert_passes();
}

//! The test environment's primary may name its schema file with
//! `schema_dump:`. Roundhouse reads one form of it: a plain or quoted YAML
//! string, written directly in the configuration, naming a relative `.rb`
//! or `.sql` file under `db/`, as Rails joins it there. Every other form,
//! and a `schema_dump` or `schema_format` in a literal `url:`'s query, is
//! an error naming the configuration and the setting, never a silent
//! choice of another file. Another database's file is a warning: the
//! emitted runtime opens only the primary.

#[path = "support/prerequisite.rs"]
mod prerequisite;
#[path = "support/scratch.rs"]
mod scratch;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use roundhouse::analyze::{diagnose, Analyzer};
use roundhouse::diagnostic::Severity;
use roundhouse::emit::diagnostics::scope;
use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::project::{target_files, BuildTarget};
use roundhouse::App;

const WIDGETS: &str = "ActiveRecord::Schema[8.1].define(version: 1) do\n  create_table :widgets, force: true do |t|\n    t.string :title, null: false\n  end\nend\n";
const STALE: &str = "ActiveRecord::Schema[8.1].define(version: 1) do\n  create_table :gadgets, force: true do |t|\n    t.string :name\n  end\nend\n";
const WIDGET: &str = "class Widget < ActiveRecord::Base\n  def label = title\nend\n";
const ROUTES: &str = "Rails.application.routes.draw do\nend\n";

/// The synthetic repro: every environment's primary names its schema file.
fn named_yml(schema_dump: &str) -> String {
    ["development", "test", "production"]
        .iter()
        .map(|env| format!("{env}:\n  primary:\n    adapter: sqlite3\n    database: storage/catalog.sqlite3\n    schema_dump: {schema_dump}\n"))
        .collect()
}

fn files(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![("app/models/widget.rb".into(), WIDGET.into()), ("config/routes.rb".into(), ROUTES.into())];
    for (path, content) in pairs {
        out.retain(|(existing, _)| existing != path);
        out.push((path.to_string(), content.to_string()));
    }
    out
}

struct Checked {
    app: App,
    /// Rendered `path:line:col: error[code]: message`.
    errors: Vec<String>,
    warnings: Vec<String>,
}

fn check(files: &[(String, String)]) -> Checked {
    let tree: HashMap<PathBuf, Vec<u8>> = files.iter().map(|(path, content)| (PathBuf::from(path), content.as_bytes().to_vec())).collect();
    let mut app = ingest_app_from_tree(tree).unwrap_or_else(|error| panic!("ingest failed: {error}"));
    Analyzer::new(&app).analyze(&mut app);
    let diagnostics = diagnose(&app);
    let rendered = |severity: Severity| diagnostics.iter().filter(|d| d.severity == severity).map(|d| d.render(&app.sources)).collect::<Vec<_>>();
    let (errors, warnings) = (rendered(Severity::Error), rendered(Severity::Warning));
    Checked { app, errors, warnings }
}

fn tables(app: &App) -> Vec<String> {
    let mut names: Vec<String> = app.schema.tables.keys().map(|name| name.as_str().to_string()).collect();
    names.sort();
    names
}

fn assert_at(diagnostics: &[String], location: &str, phrases: &[&str]) {
    let hit = diagnostics.iter().any(|d| d.starts_with(location) && phrases.iter().all(|p| d.contains(p)));
    assert!(hit, "expected a diagnostic at {location} naming {phrases:?}; got:\n{}", diagnostics.join("\n"));
}

fn spinel_tree(app: &App) -> Vec<(String, String)> {
    let (result, diagnostics) = scope(|| target_files(app, std::path::Path::new("not-a-fixture"), BuildTarget::Spinel));
    result.unwrap_or_else(|error| panic!("spinel emit failed: {error}\n{diagnostics:?}"))
}

fn file<'a>(tree: &'a [(String, String)], path: &str) -> &'a str {
    &tree.iter().find(|(p, _)| p == path).unwrap_or_else(|| panic!("no {path} emitted")).1
}

#[test]
fn the_schema_file_schema_dump_names_is_the_primary_schema() {
    let named = check(&files(&[("config/database.yml", &named_yml("catalog_schema.rb")), ("db/catalog_schema.rb", WIDGETS)]));
    assert!(named.errors.is_empty(), "{}", named.errors.join("\n"));
    assert_eq!(tables(&named.app), ["widgets"]);
    let tree = spinel_tree(&named.app);
    assert!(file(&tree, "config/schema.rb").contains("CREATE TABLE IF NOT EXISTS widgets"), "{}", file(&tree, "config/schema.rb"));
    assert!(file(&tree, "app/models/widget.rb").contains("def title\n"), "{}", file(&tree, "app/models/widget.rb"));

    // The same app with the schema at Rails' default name emits the same tree.
    let control = check(&files(&[
        ("config/database.yml", &named_yml("catalog_schema.rb").replace("    schema_dump: catalog_schema.rb\n", "")),
        ("db/schema.rb", WIDGETS),
    ]));
    assert!(control.errors.is_empty(), "{}", control.errors.join("\n"));
    let control_tree = spinel_tree(&control.app);
    assert_eq!(tree.len(), control_tree.len());
    for ((path, text), (control_path, control_text)) in tree.iter().zip(&control_tree) {
        assert_eq!(path, control_path);
        assert!(text == control_text, "{path} differs from the db/schema.rb control");
    }
}

/// Also beside the assets initializer `rails new` writes, whose comment
/// names Sprockets' and Propshaft's `assets.paths`, not Rails' `paths`.
#[test]
fn the_named_file_wins_over_a_default_one() {
    let checked = check(&files(&[
        ("config/database.yml", &named_yml("catalog_schema.rb")),
        ("db/catalog_schema.rb", WIDGETS),
        ("db/schema.rb", STALE),
        ("config/initializers/assets.rb", "Rails.application.config.assets.version = \"1.0\"\n# Rails.application.config.assets.paths << Emoji.images_path\n"),
    ]));
    assert!(checked.errors.is_empty(), "{}", checked.errors.join("\n"));
    assert_eq!(tables(&checked.app), ["widgets"]);
}

#[test]
fn a_named_file_that_is_missing_is_an_error_naming_it() {
    let checked = check(&files(&[("config/database.yml", &named_yml("catalog_schema.rb"))]));
    assert_at(&checked.errors, "config/database.yml:10:", &["`test.primary`", "db/catalog_schema.rb", "does not exist"]);
    assert!(tables(&checked.app).is_empty());

    // Rails joins a relative name to db/, so `db/x.rb` is db/db/x.rb: `schema_dump_path` keeps a
    // name only when `File.dirname(name) == db_dir`, and `db_dir` is `paths["db"].first`, which is
    // absolute. Checked against an initialized Rails 8.1.4 app.
    let prefixed = check(&files(&[("config/database.yml", &named_yml("db/catalog_schema.rb")), ("db/catalog_schema.rb", WIDGETS)]));
    assert_at(&prefixed.errors, "config/database.yml:10:", &["`schema_dump: db/catalog_schema.rb`", "db/db/catalog_schema.rb", "does not exist"]);
}

#[test]
fn another_databases_schema_file_is_reported_and_not_carried() {
    let yml = "test:\n  primary:\n    adapter: sqlite3\n    database: storage/test.sqlite3\n  catalog:\n    adapter: sqlite3\n    database: storage/catalog.sqlite3\n    schema_dump: catalog_schema.rb\n  audit:\n    adapter: sqlite3\n    database: storage/audit.sqlite3\n  primary_replica:\n    adapter: sqlite3\n    database: storage/test.sqlite3\n    replica: true\n  cache:\n    adapter: sqlite3\n    database: storage/cache.sqlite3\n    schema_dump: false\nproduction:\n  primary:\n    adapter: sqlite3\n    database: storage/production.sqlite3\n  reports:\n    adapter: sqlite3\n    database: storage/reports.sqlite3\n";
    let checked = check(&files(&[
        ("config/database.yml", yml),
        ("db/schema.rb", STALE),
        ("db/catalog_schema.rb", WIDGETS),
        ("db/audit_schema.rb", WIDGETS),
    ]));
    assert!(checked.errors.is_empty(), "{}", checked.errors.join("\n"));
    assert_eq!(tables(&checked.app), ["gadgets"], "another database's tables joined the primary's");
    assert_at(&checked.warnings, "config/database.yml:5:", &["`test.catalog`", "db/catalog_schema.rb", "opens only the primary"]);
    assert_at(&checked.warnings, "config/database.yml:9:", &["`test.audit`", "db/audit_schema.rb"]);
    let reported: Vec<&String> = checked.warnings.iter().filter(|w| w.contains("schema_dump")).collect();
    assert_eq!(reported.len(), 2, "a replica, `schema_dump: false` and another environment are not reported: {reported:#?}");

    // Without its file there are no tables to leave out.
    let without = check(&files(&[("config/database.yml", yml), ("db/schema.rb", STALE), ("db/catalog_schema.rb", WIDGETS)]));
    let reported: Vec<&String> = without.warnings.iter().filter(|w| w.contains("schema_dump")).collect();
    assert_eq!(reported.len(), 1, "{reported:#?}");
}

#[test]
fn a_structure_dump_is_read_under_a_literal_sql_format() {
    let structure = "CREATE TABLE public.widgets (\n    id bigint NOT NULL,\n    title character varying NOT NULL\n);\n";
    let yml = "test:\n  adapter: postgresql\n  database: app_test\n  schema_format: sql\n  schema_dump: catalog_structure.sql\n";
    let checked = check(&files(&[("config/database.yml", yml), ("db/catalog_structure.sql", structure), ("db/schema.rb", STALE)]));
    assert!(checked.errors.is_empty(), "{}", checked.errors.join("\n"));
    assert_eq!(tables(&checked.app), ["widgets"]);
}

/// Each form of `schema_dump` this does not read is an error naming the
/// configuration and the setting; the schema stays empty rather than
/// falling back to db/schema.rb.
#[test]
fn every_other_form_is_refused_by_name() {
    let base = "test:\n  adapter: sqlite3\n  database: storage/test.sqlite3\n";
    let cases: &[(&str, String, &[(&str, &str)], &[&str])] = &[
        ("ERB", format!("{base}  schema_dump: <%= ENV[\"SCHEMA_FILE\"] %>\n"), &[], &["SCHEMA_FILE", "ERB"]),
        ("no extension", format!("{base}  schema_dump: 'off'\n"), &[("db/off", WIDGETS)], &["`schema_dump: 'off'`", ".rb or .sql"]),
        ("absolute", format!("{base}  schema_dump: /srv/catalog_schema.rb\n"), &[], &["/srv/catalog_schema.rb", "relative path"]),
        ("outside db/", format!("{base}  schema_dump: ../catalog_schema.rb\n"), &[("catalog_schema.rb", WIDGETS)], &["../catalog_schema.rb", "relative path"]),
        ("Symbol key", format!("{base}  :schema_dump: catalog_schema.rb\n"), &[("db/catalog_schema.rb", WIDGETS)], &["Symbol key"]),
        ("anchor", format!("{base}  schema_dump: &file catalog_schema.rb\n"), &[("db/catalog_schema.rb", WIDGETS)], &["anchor"]),
        ("merge", format!("default: &default\n  schema_dump: catalog_schema.rb\ntest:\n  <<: *default\n  adapter: sqlite3\n"), &[("db/catalog_schema.rb", WIDGETS)], &["anchor merge or flow"]),
        ("flow", "test: { adapter: sqlite3, schema_dump: catalog_schema.rb }\n".to_string(), &[("db/catalog_schema.rb", WIDGETS)], &["anchor merge or flow"]),
        ("URL schema_dump", format!("{base}  url: 'sqlite3:///tmp/test.sqlite3?schema_dump=catalog_schema.rb'\n"), &[("db/catalog_schema.rb", WIDGETS)], &["`url:`", "`schema_dump`"]),
        ("URL schema_format", format!("{base}  url: 'sqlite3:///tmp/test.sqlite3?schema_format=sql'\n"), &[], &["`url:`", "`schema_format`"]),
        ("paths[\"db\"]", format!("{base}  schema_dump: catalog_schema.rb\n"), &[("db/catalog_schema.rb", WIDGETS), ("config/application.rb", "class App < Rails::Application\n  config.paths[\"db\"] = [\"database\"]\nend\n")], &["config/application.rb", "mentions `.paths`"]),
        ("spaced paths[ \"db\" ]", format!("{base}  schema_dump: catalog_schema.rb\n"), &[("db/catalog_schema.rb", WIDGETS), ("config/application.rb", "class App < Rails::Application\n  config.paths[ \"db\" ] = [\"database\"]\nend\n")], &["config/application.rb", "mentions `.paths`"]),
        ("paths.add(\"db\")", format!("{base}  schema_dump: catalog_schema.rb\n"), &[("db/catalog_schema.rb", WIDGETS), ("config/application.rb", "class App < Rails::Application\n  config.paths.add(\"db\", with: \"database\")\nend\n")], &["config/application.rb", "mentions `.paths`"]),
        ("aliased paths", format!("{base}  schema_dump: catalog_schema.rb\n"), &[("db/catalog_schema.rb", WIDGETS), ("config/application.rb", "class App < Rails::Application\n  paths = config.paths\n  paths[ 'db' ] = ['database']\nend\n")], &["config/application.rb", "mentions `.paths`"]),
        ("aliased receiver", format!("{base}  schema_dump: catalog_schema.rb\n"), &[("db/catalog_schema.rb", WIDGETS), ("config/application.rb", "class App < Rails::Application\n  p = config.paths\n  p[\"db\"] = [\"database\"]\nend\n")], &["config/application.rb", "mentions `.paths`"]),
        ("computed paths key", format!("{base}  schema_dump: catalog_schema.rb\n"), &[("db/catalog_schema.rb", WIDGETS), ("config/application.rb", "class App < Rails::Application\n  key = \"db\"\n  config.paths[key] = [\"database\"]\nend\n")], &["config/application.rb", "mentions `.paths`"]),
        ("quoted ':url' beside url", format!("{base}  schema_dump: catalog_schema.rb\n  url: 'sqlite3:///tmp/test.sqlite3'\n  ':url': 'sqlite3:///tmp/test.sqlite3'\n"), &[("db/catalog_schema.rb", WIDGETS)], &["more than one spelling"]),
        ("merged :url beside url", format!("defaults: &defaults\n  :url: 'sqlite3:///tmp/test.sqlite3'\ntest:\n  <<: *defaults\n  adapter: sqlite3\n  schema_dump: catalog_schema.rb\n  url: 'sqlite3:///tmp/test.sqlite3'\n"), &[("db/catalog_schema.rb", WIDGETS)], &["more than one spelling"]),
        ("ERB if around the environment", "<% if ENV[\"CI\"] %>\ntest:\n  adapter: sqlite3\n  schema_dump: catalog_schema.rb\n<% end %>\n".to_string(), &[("db/catalog_schema.rb", WIDGETS)], &["ERB control flow"]),
        ("ERB if around the key", format!("{base}<% if ENV[\"CI\"] %>\n  schema_dump: catalog_schema.rb\n<% end %>\n"), &[("db/catalog_schema.rb", WIDGETS)], &["ERB control flow"]),
        ("environment URL string", "test: 'sqlite3:///tmp/test.sqlite3?schema_dump=catalog_schema.rb'\n".to_string(), &[("db/catalog_schema.rb", WIDGETS)], &["`url:`", "`schema_dump`"]),
        ("`:url:` query", format!("{base}  schema_dump: catalog_schema.rb\n  :url: 'sqlite3:///tmp/test.sqlite3?schema_dump=other.rb'\n"), &[("db/catalog_schema.rb", WIDGETS), ("db/other.rb", WIDGETS)], &["`url:`", "`schema_dump`"]),
        ("computed url", format!("{base}  schema_dump: catalog_schema.rb\n  url: <%= ENV[\"DATABASE_URL\"] %>\n"), &[("db/catalog_schema.rb", WIDGETS)], &["`url:` is computed"]),
        ("`.rb` under schema_format: sql", format!("{base}  schema_format: sql\n  schema_dump: catalog.rb\n"), &[("db/catalog.rb", "CREATE TABLE gadgets (id integer PRIMARY KEY, title varchar);")], &["extension"]),
        ("`.rb` under schema_format: sql, widgets", format!("{base}  schema_format: sql\n  schema_dump: catalog.rb\n"), &[("db/catalog.rb", "CREATE TABLE widgets (id integer PRIMARY KEY, title varchar);")], &["extension"]),
        ("`.sql` without schema_format", format!("{base}  schema_dump: catalog.sql\n"), &[("db/catalog.sql", "CREATE TABLE widgets (id integer PRIMARY KEY, title varchar);")], &["extension"]),
        ("app-level schema_format", format!("{base}  schema_dump: catalog_schema.rb\n"), &[("db/catalog_schema.rb", WIDGETS), ("config/application.rb", "class App < Rails::Application\n  config.active_record.schema_format = :ruby\nend\n")], &["config/application.rb", "mentions `schema_format`"]),
    ];
    let mut failures = Vec::new();
    for (name, yml, extra, phrases) in cases {
        let mut pairs: Vec<(&str, &str)> = vec![("config/database.yml", yml.as_str()), ("db/schema.rb", STALE)];
        pairs.extend_from_slice(extra);
        let checked = check(&files(&pairs));
        let hit = checked.errors.iter().any(|e| e.contains("`test.primary`") && phrases.iter().all(|p| e.contains(p)));
        if !hit || !tables(&checked.app).is_empty() {
            failures.push(format!("{name}: tables {:?}, errors {:?}", tables(&checked.app), checked.errors));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The app's likely shape: a three-tier test environment whose other
/// databases set their own `schema_dump`, `schema_format` and `url`. Only
/// the primary's keys decide its file; the others keep their warning.
#[test]
fn the_primary_reads_its_file_beside_other_databases() {
    let yml = "test:\n  primary:\n    adapter: sqlite3\n    database: storage/test.sqlite3\n    schema_dump: catalog_schema.rb\n  archive:\n    adapter: sqlite3\n    database: storage/archive.sqlite3\n    schema_format: sql\n    schema_dump: archive_structure.sql\n  cache:\n    adapter: sqlite3\n    url: 'sqlite3:///tmp/cache.sqlite3?schema_dump=other.rb'\n";
    let checked = check(&files(&[
        ("config/database.yml", yml),
        ("db/catalog_schema.rb", WIDGETS),
        ("db/schema.rb", STALE),
        ("db/archive_structure.sql", "CREATE TABLE archived (id integer PRIMARY KEY);\n"),
    ]));
    assert!(checked.errors.is_empty(), "{}", checked.errors.join("\n"));
    assert_eq!(tables(&checked.app), ["widgets"]);
    assert_at(&checked.warnings, "config/database.yml:6:", &["`test.archive`", "db/archive_structure.sql", "opens only the primary"]);
}

/// A quoted environment or configuration key is located like a plain one.
#[test]
fn quoted_environment_and_configuration_keys_are_located() {
    for yml in [
        "'test':\n  adapter: sqlite3\n  schema_dump: catalog_schema.rb\n",
        "\"test\":\n  adapter: sqlite3\n  schema_dump: catalog_schema.rb\n",
        "test:\n  'primary':\n    adapter: sqlite3\n    schema_dump: catalog_schema.rb\n  archive:\n    adapter: sqlite3\n",
    ] {
        let checked = check(&files(&[("config/database.yml", yml), ("db/catalog_schema.rb", WIDGETS), ("db/schema.rb", STALE)]));
        assert!(checked.errors.is_empty(), "{yml}: {}", checked.errors.join("\n"));
        assert_eq!(tables(&checked.app), ["widgets"], "{yml}");
    }
}

/// solid_queue's records are found from the named primary schema too.
#[test]
fn queue_tables_in_the_named_schema_select_solid_queue_models() {
    let checked = check(&files(&[
        ("config/database.yml", "test:\n  adapter: sqlite3\n  schema_dump: catalog_schema.rb\n"),
        ("db/catalog_schema.rb", "ActiveRecord::Schema[8.1].define(version: 1) do\n  create_table :solid_queue_jobs do |t|\n    t.text :arguments\n  end\nend\n"),
        ("config/application.rb", "config.active_job.queue_adapter = :solid_queue"),
        ("app/models/queue_stats.rb", "class QueueStats; def self.jobs; SolidQueue::Job.count; end; end"),
        (".bundle/config", "BUNDLE_PATH: vendor/bundle"),
        ("Gemfile.lock", "GEM\n  remote: https://rubygems.org/\n  specs:\n    solid_queue (1.7.0)\n\nDEPENDENCIES\n  solid_queue\n"),
        ("vendor/bundle/ruby/3.4.0/gems/solid_queue-1.7.0/app/models/solid_queue/job.rb", "class SolidQueue::Job < ActiveRecord::Base; self.table_name = 'solid_queue_jobs'; end"),
    ]));
    assert_eq!(tables(&checked.app), ["solid_queue_jobs"]);
    assert!(checked.app.models.iter().any(|m| m.name.0.as_str() == "SolidQueue::Job"), "SolidQueue::Job was not selected");
}

/// A three-tier test environment whose configurations are named by
/// non-String keys has no primary this can name: a named file there is
/// refused, and ingestion doesn't panic.
#[test]
fn a_primary_that_cannot_be_named_refuses_a_named_file() {
    let checked = check(&files(&[("config/database.yml", "test:\n  1:\n    adapter: sqlite3\n    schema_dump: catalog_schema.rb\n"), ("db/catalog_schema.rb", WIDGETS), ("db/schema.rb", STALE)]));
    assert_at(&checked.errors, "config/database.yml:1:", &["`test.primary`", "can't be named"]);
    assert!(tables(&checked.app).is_empty());
}

/// A blank final value drops the key from Rails' URL options, and a quoted
/// `':schema_dump'` is a String key Rails ignores. `schema_dump: false`,
/// null or an unquoted YAML 1.1 boolean names no file. All build as before.
#[test]
fn these_forms_build_as_before() {
    let base = "test:\n  adapter: sqlite3\n  database: storage/test.sqlite3\n";
    for yml in [
        format!("{base}  url: 'sqlite3:///tmp/test.sqlite3?schema_format=sql&schema_format='\n"),
        format!("{base}  url: 'sqlite3:///tmp/test.sqlite3?schema_dump=catalog_schema.rb&schema_dump'\n"),
        format!("{base}  ':schema_dump': catalog_schema.rb\n"),
        format!("{base}  schema_dump: false\n"),
        format!("{base}  schema_dump: ~\n"),
        format!("{base}  schema_dump: off\n"),
        format!("{base}  schema_dump: No\n"),
        format!("default: &default\n  schema_dump: false\ntest:\n  <<: *default\n  adapter: sqlite3\n"),
        format!("{base}  schema_format: sql\n"),
        "development:\n  adapter: sqlite3\n  schema_dump: catalog_schema.rb\ntest:\n  adapter: sqlite3\n".to_string(),
    ] {
        let checked = check(&files(&[("config/database.yml", &yml), ("db/schema.rb", WIDGETS), ("db/catalog_schema.rb", STALE)]));
        assert!(checked.errors.is_empty(), "{yml}: {}", checked.errors.join("\n"));
        assert_eq!(tables(&checked.app), ["widgets"], "{yml}");
    }
}

fn ruby_with_sqlite3() -> bool {
    Command::new("ruby").args(["-rsqlite3", "-e", "1"]).output().is_ok_and(|o| o.status.success())
}

/// What the report ran: the emitted DDL, then an insert, through the
/// sqlite3 gem.
#[test]
fn the_emitted_ddl_takes_an_insert() {
    if !ruby_with_sqlite3() {
        prerequisite::missing("ruby with the sqlite3 gem");
        return;
    }
    let named = check(&files(&[("config/database.yml", &named_yml("catalog_schema.rb")), ("db/catalog_schema.rb", WIDGETS)]));
    let dir = scratch::dir("roundhouse-schema-dump");
    std::fs::write(dir.join("schema.rb"), file(&spinel_tree(&named.app), "config/schema.rb")).unwrap();
    let run = Command::new("ruby")
        .arg("-e")
        .arg(
            "require 'sqlite3'\nload ARGV[0]\ndb = SQLite3::Database.new(':memory:')\nSchema.statements.each { |sql| db.execute(sql) }\n\
             db.execute(\"INSERT INTO widgets (title) VALUES ('kept')\")\nputs db.get_first_value('SELECT title FROM widgets')\n\
             puts db.execute('PRAGMA table_info(widgets)').select { |row| row[5] == 0 }.map { |row| \"#{row[1]}:#{row[3]}\" }.join(' ')\n",
        )
        .arg(dir.join("schema.rb"))
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    // Each column but the key, with its NOT NULL: the same in the Rails control below.
    assert_eq!(String::from_utf8_lossy(&run.stdout), "kept\ntitle:1\n");
}


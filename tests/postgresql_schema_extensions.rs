//! PostgreSQL: `enable_extension` and `create_schema` in db/schema.rb
//! reach the DDL the emitted app runs at boot (`Schema.statements`), as
//! Rails' `db:schema:load` runs them ahead of the tables. SQLite has no
//! extensions, and its DDL is unchanged.
//!
//! The emitted-tree checks need no server. The boot run reads
//! ROUNDHOUSE_PG_TEST_URL through tests/support/pg_test_url.rs, skipping
//! without it unless ROUNDHOUSE_PG_TEST_REQUIRED is set, and needs the pg
//! gem and a server that ships pgcrypto.
//!
//!   ROUNDHOUSE_PG_TEST_URL=postgres://postgres@127.0.0.1:5432/postgres \
//!   cargo test --test postgresql_schema_extensions

#[path = "support/emit_and_run.rs"]
mod emit_and_run;
#[path = "support/pg_test_url.rs"]
mod pg_test_url;

use roundhouse::project::BuildTarget;

const POSTGRESQL_DATABASE_YML: &str = "default: &default\n  adapter: postgresql\n  encoding: unicode\n\ndevelopment:\n  <<: *default\n  database: app_development\n\ntest:\n  <<: *default\n  database: app_test\n";

const WIDGET: &str = r#"class Widget < ApplicationRecord
  def self.digest_of(text)
    connection.select_value(sanitize_sql_array(["SELECT encode(digest(?, 'sha256'), 'hex')", text]))
  end
end
"#;

fn schema_rb(preamble: &str) -> String {
    format!(
        "ActiveRecord::Schema[8.1].define(version: 1) do\n{preamble}\n  create_table \"widgets\", force: :cascade do |t|\n    t.string \"name\"\n  end\nend\n"
    )
}

fn app(database_yml: Option<&str>, schema: &str) -> emit_and_run::Overlay {
    let app = emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/widget.rb", WIDGET)
        .write("db/schema.rb", schema);
    match database_yml {
        Some(yml) => app.write("config/database.yml", yml),
        None => app,
    }
}

fn read(emitted: &std::path::Path, path: &str) -> String {
    std::fs::read_to_string(emitted.join(path)).unwrap_or_else(|error| panic!("{path}: {error}"))
}

#[test]
fn postgresql_boot_ddl_creates_the_schemas_extensions_ahead_of_the_tables() {
    let schema = schema_rb("  create_schema \"extensions\"\n  enable_extension \"extensions.pgcrypto\"\n  enable_extension \"citext\"");
    let (emitted, errors) = app(Some(POSTGRESQL_DATABASE_YML), &schema).emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let ddl = read(&emitted, "config/schema.rb");
    let at = |needle: &str| ddl.find(needle).unwrap_or_else(|| panic!("no `{needle}` in:\n{ddl}"));
    let create_schema = at(r#"CREATE SCHEMA IF NOT EXISTS \"extensions\""#);
    let pgcrypto = at(r#"CREATE EXTENSION IF NOT EXISTS \"pgcrypto\" SCHEMA extensions"#);
    let citext = at(r#"CREATE EXTENSION IF NOT EXISTS \"citext\""#);
    let table = at(r#"CREATE TABLE IF NOT EXISTS \"widgets\""#);
    assert!(create_schema < pgcrypto && pgcrypto < citext && citext < table, "{ddl}");
}

#[test]
fn a_disabled_extension_is_not_created() {
    let schema = schema_rb("  enable_extension \"pgcrypto\"\n  enable_extension \"citext\"\n  disable_extension \"pgcrypto\"");
    let (emitted, errors) = app(Some(POSTGRESQL_DATABASE_YML), &schema).emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let ddl = read(&emitted, "config/schema.rb");
    assert!(ddl.contains(r#"CREATE EXTENSION IF NOT EXISTS \"citext\""#), "{ddl}");
    assert!(!ddl.contains("pgcrypto"), "{ddl}");
}

#[test]
fn sqlite_boot_ddl_has_no_extensions() {
    let schema = schema_rb("  create_schema \"extensions\"\n  enable_extension \"extensions.pgcrypto\"");
    let (emitted, errors) = app(None, &schema).emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let ddl = read(&emitted, "config/schema.rb");
    assert!(ddl.contains("CREATE TABLE IF NOT EXISTS"), "{ddl}");
    assert!(!ddl.contains("EXTENSION") && !ddl.contains("CREATE SCHEMA"), "{ddl}");
}

/// What a probe prints: pgcrypto's `digest` through the model, and the
/// schema the extension went into, named relative to the test's schema.
const PROBE: &str = r#"puts Widget.digest_of("abc")
row = Widget.connection.select_rows("SELECT n.nspname FROM pg_extension e JOIN pg_namespace n ON n.oid = e.extnamespace WHERE e.extname = 'pgcrypto'")
puts row.map { |r| r[0].sub(ENV.fetch("RH_TEST_SCHEMA"), "<schema>") }.inspect
"#;

/// The probe on ActiveRecord 8.1.4 after `db:schema:load` of `schema`,
/// with `search_path` as `schema_search_path`. `None` when this Ruby has
/// no Rails 8.1.4 or no pg gem for it.
fn rails_answers(url: &str, search_path: &str, schema: &str, test_schema: &str) -> Option<String> {
    let oracle = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/rails_oracle.rb");
    let program = format!(
        "require {oracle:?}\n\
         unless RailsOracle.available?(\"activerecord\", pg: \">= 1.1\")\n  warn \"skipped the Rails control: #{{RailsOracle.label}} or its driver is not installed\"\n  exit 77\nend\n\
         RailsOracle.activate(\"activerecord\", pg: \">= 1.1\")\nrequire \"active_record\"\n\
         ActiveRecord::Base.establish_connection(url: {url:?}, schema_search_path: {search_path:?})\n\
         ActiveRecord::Schema.verbose = false\n{schema}\
         class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n{WIDGET}\
         ENV[\"RH_TEST_SCHEMA\"] = {test_schema:?}\n{PROBE}"
    );
    let output = emit_and_run::ruby().args(["-e", &program]).output().expect("ruby");
    if output.status.code() == Some(77) {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        return None;
    }
    assert!(output.status.success(), "Rails control failed:\n{}", String::from_utf8_lossy(&output.stderr));
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Boot (`Main.configure_default_adapter!`) on an empty schema creates
/// the extension, so the model's SQL reaches its function, as it does on
/// Rails after `db:schema:load`. Both forms run in this one test, one
/// after the other: an extension is one per database, so two tests
/// creating pgcrypto at once would race. Rails runs first on each form,
/// in a schema of its own, which is dropped (with the extension) before
/// the emitted app boots.
#[test]
fn boot_creates_the_extension_the_models_sql_calls() {
    let Some(db) = pg_test_url::pg_test_db("postgresql_schema_extensions") else { return };
    let on_server = |sql: String| {
        let output = std::process::Command::new("ruby")
            .args(["-rpg", "-e", "PG.connect(ARGV[0]).exec(ARGV[1])", &db.url, &sql])
            .output()
            .expect("ruby with the pg gem");
        assert!(output.status.success(), "{sql}: {}", String::from_utf8_lossy(&output.stderr));
    };
    // `enable_extension "pgcrypto"` goes into the first schema on the
    // search path; `extensions.pgcrypto` into its own schema, which the
    // search path then names.
    let cases = [("plain", false), ("qualified", true)];
    for (label, qualified) in cases {
        for side in ["rails", "emitted"] {
            let schema = format!("{}_{side}", db.schema);
            let ext_schema = format!("{schema}_ext");
            let (preamble, search_path) = if qualified {
                (format!("  create_schema \"{ext_schema}\"\n  enable_extension \"{ext_schema}.pgcrypto\""), format!("{schema},{ext_schema}"))
            } else {
                ("  enable_extension \"pgcrypto\"".to_string(), schema.clone())
            };
            let schema_rb = schema_rb(&preamble);
            on_server(format!("CREATE SCHEMA {schema}"));
            let drop = || on_server(format!("DROP SCHEMA IF EXISTS {schema} CASCADE; DROP SCHEMA IF EXISTS {ext_schema} CASCADE"));
            let answers = if side == "rails" {
                let answers = rails_answers(&db.url, &search_path, &schema_rb, &schema);
                drop();
                answers
            } else {
                let options = format!("-c search_path={search_path}");
                let run = app(Some(POSTGRESQL_DATABASE_YML), &schema_rb).run_ruby_env(
                    PROBE,
                    &[("DATABASE_URL", db.url.as_str()), ("PGOPTIONS", options.as_str()), ("RH_TEST_SCHEMA", schema.as_str())],
                );
                drop();
                run.assert_passes();
                Some(run.stdout.clone())
            };
            let Some(answers) = answers else { continue };
            let expected = format!(
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\n[\"<schema>{}\"]\n",
                if qualified { "_ext" } else { "" }
            );
            assert_eq!(answers, expected, "{label} on {side}");
        }
    }
}

/// Folds one migration's `change` body, as the db/migrate walk does with
/// no db/schema.rb, and returns the PostgreSQL boot DDL's schema and
/// extension statements, in order.
fn folded_database_objects(body: &str) -> Result<Vec<String>, String> {
    let mut schema = roundhouse::schema::Schema::default();
    let source = format!("class Extensions < ActiveRecord::Migration[8.1]\n  def change\n{body}\n  end\nend\n");
    roundhouse::ingest::ingest_migration_with_generated_expression_dialect(
        source.as_bytes(),
        "db/migrate/20260101000000_extensions.rb",
        &mut schema,
        roundhouse::schema::generated::GeneratedExpressionDialect::Postgres,
    )
    .map_err(|error| error.to_string())?;
    let ddl = roundhouse::emit::shared::schema_sql::render_schema_statements_for(&schema, roundhouse::emit::shared::schema_sql::Dialect::Postgres)?;
    Ok(ddl.into_iter().filter(|sql| sql.contains("EXTENSION") || sql.contains("CREATE SCHEMA")).collect())
}

/// A repeated `enable_extension` is Rails' `CREATE EXTENSION IF NOT
/// EXISTS` again, a no-op: ActiveRecord 8.1.4.1 leaves `cube` installed
/// ahead of `earthdistance`, which requires it, and leaves `pgcrypto` in
/// `public` when `extensions.pgcrypto` follows. After a disable, enabling
/// again installs it last.
#[test]
fn a_repeated_enable_keeps_the_first_position_and_schema() {
    let ddl = folded_database_objects("    enable_extension \"cube\"\n    enable_extension \"earthdistance\"\n    enable_extension \"cube\"");
    assert_eq!(ddl, Ok(vec![r#"CREATE EXTENSION IF NOT EXISTS "cube""#.to_string(), r#"CREATE EXTENSION IF NOT EXISTS "earthdistance""#.to_string()]));
    let ddl = folded_database_objects("    create_schema \"extensions\"\n    enable_extension \"pgcrypto\"\n    enable_extension \"extensions.pgcrypto\"");
    assert_eq!(ddl, Ok(vec![r#"CREATE SCHEMA IF NOT EXISTS "extensions""#.to_string(), r#"CREATE EXTENSION IF NOT EXISTS "pgcrypto""#.to_string()]));
    let ddl = folded_database_objects("    enable_extension \"cube\"\n    enable_extension \"pgcrypto\"\n    disable_extension \"cube\"\n    enable_extension \"cube\"");
    assert_eq!(ddl, Ok(vec![r#"CREATE EXTENSION IF NOT EXISTS "pgcrypto""#.to_string(), r#"CREATE EXTENSION IF NOT EXISTS "cube""#.to_string()]));
}

/// `disable_extension "cube", force: :cascade` is `DROP EXTENSION IF
/// EXISTS "cube" CASCADE`, which also drops what depends on it: other
/// extensions (`earthdistance`) and columns of its types (a `citext`
/// column). The fold knows neither, so it refuses the migration. A plain
/// disable still folds.
#[test]
fn a_cascading_disable_is_refused() {
    let error = folded_database_objects("    enable_extension \"cube\"\n    enable_extension \"earthdistance\"\n    disable_extension \"cube\", force: :cascade")
        .expect_err("a cascading disable is refused");
    assert!(error.contains("disable_extension") && error.contains("force: :cascade"), "{error}");
    let ddl = folded_database_objects("    enable_extension \"cube\"\n    enable_extension \"pgcrypto\"\n    disable_extension \"cube\", force: false");
    assert_eq!(ddl, Ok(vec![r#"CREATE EXTENSION IF NOT EXISTS "pgcrypto""#.to_string()]));
}

/// `drop_schema` is `DROP SCHEMA ... CASCADE`: ActiveRecord 8.1.4.1
/// drops the extension created in that schema with it, and keeps one in
/// another schema.
#[test]
fn drop_schema_drops_the_extensions_created_in_it() {
    let ddl = folded_database_objects("    create_schema \"extensions\"\n    enable_extension \"extensions.pgcrypto\"\n    enable_extension \"cube\"\n    drop_schema \"extensions\"");
    assert_eq!(ddl, Ok(vec![r#"CREATE EXTENSION IF NOT EXISTS "cube""#.to_string()]));
}

//! PostgreSQL: `enable_extension` and `create_schema` in db/schema.rb
//! reach the DDL the emitted app runs at boot (`Schema.statements`), as
//! Rails' `db:schema:load` runs them ahead of the tables. SQLite has no
//! extensions, and its DDL is unchanged.
//!
//! The emitted-tree checks need no server. The boot run reads
//! ROUNDHOUSE_PG_TEST_URL through tests/support/pg_test_url.rs, skipping
//! without it unless ROUNDHOUSE_PG_TEST_REQUIRED is set, and needs the pg
//! gem, a server that ships pgcrypto, and a role that may create
//! databases: the run makes one of its own and drops it.
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

/// Runs `sql` against `url`, or fails the test with the server's error.
fn exec(url: &str, sql: &str) {
    let output = std::process::Command::new("ruby")
        .args(["-rpg", "-e", "PG.connect(ARGV[0]).exec(ARGV[1])", url, sql])
        .output()
        .expect("ruby with the pg gem");
    assert!(output.status.success(), "{sql}: {}", String::from_utf8_lossy(&output.stderr));
}

/// `url` with its database replaced by `name`, keeping the query but for
/// a `dbname` in it, which libpq would read over the path.
fn with_database(url: &str, name: &str) -> String {
    let (base, query) = url.split_once('?').unwrap_or((url, ""));
    let (scheme, rest) = base.split_once("://").expect("a postgres:// URL");
    let authority = rest.split_once('/').map_or(rest, |(authority, _)| authority);
    let query: Vec<&str> = query.split('&').filter(|pair| !pair.is_empty() && pair.split('=').next() != Some("dbname")).collect();
    let query = if query.is_empty() { String::new() } else { format!("?{}", query.join("&")) };
    format!("{scheme}://{authority}/{name}{query}")
}

/// The rebuilt URL names the run's database whatever the configured one
/// looks like: with or without a path, and with a query, which libpq
/// reads after the path, so a `dbname` there would win over it.
#[test]
fn the_own_database_url_names_only_the_run_s_database() {
    let cases = [
        ("postgres://u:p@h:5432/db", "postgres://u:p@h:5432/own"),
        ("postgres://u@h:5432/db?sslmode=disable", "postgres://u@h:5432/own?sslmode=disable"),
        ("postgres://postgres@localhost", "postgres://postgres@localhost/own"),
        ("postgres://postgres@localhost?connect_timeout=5", "postgres://postgres@localhost/own?connect_timeout=5"),
        ("postgres://h/db?dbname=postgres&sslmode=disable", "postgres://h/own?sslmode=disable"),
        ("postgres://h?dbname=postgres", "postgres://h/own"),
    ];
    for (configured, own) in cases {
        assert_eq!(with_database(configured, "own"), own, "{configured}");
    }
}

/// A database of the run's own on the configured server, created from
/// `template0` and dropped when this is dropped, also while a failed
/// assertion unwinds. An extension is one per database, so what the run
/// creates or drops, `pgcrypto` and what depends on it, never reaches the
/// configured database or a concurrent run's.
struct OwnDatabase {
    admin_url: String,
    name: String,
    url: String,
}

impl OwnDatabase {
    fn create(admin_url: &str, name: &str) -> Self {
        exec(admin_url, &format!("CREATE DATABASE \"{name}\" TEMPLATE template0"));
        OwnDatabase { admin_url: admin_url.to_string(), name: name.to_string(), url: with_database(admin_url, name) }
    }
}

impl Drop for OwnDatabase {
    fn drop(&mut self) {
        // No assertion here: this also runs while a failed assertion unwinds.
        // FORCE ends a connection the emitted app left open.
        let sql = format!("DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)", self.name);
        let _ = std::process::Command::new("ruby").args(["-rpg", "-e", "PG.connect(ARGV[0]).exec(ARGV[1])", &self.admin_url, &sql]).output();
    }
}

/// Runs `sql` against `url` when dropped, so a run that panics still
/// removes its schemas, and the extension in them, before the next case.
struct DropOnExit<'a> {
    url: &'a str,
    sql: String,
}

impl Drop for DropOnExit<'_> {
    fn drop(&mut self) {
        // No assertion here: this also runs while a failed assertion unwinds.
        let _ = std::process::Command::new("ruby").args(["-rpg", "-e", "PG.connect(ARGV[0]).exec(ARGV[1])", self.url, &self.sql]).output();
    }
}

/// Boot (`Main.configure_default_adapter!`) on an empty schema creates
/// the extension, so the model's SQL reaches its function, as it does on
/// Rails after `db:schema:load`. The run has a database of its own
/// ([`OwnDatabase`]), so a pgcrypto the configured database has, and what
/// depends on it there, is neither read nor dropped. Both forms run in
/// this one test, one after the other, as the extension is one per
/// database. Rails runs first on each form, in a schema of its own, which
/// is dropped (with the extension) before the emitted app boots, also when
/// the run fails.
#[test]
fn boot_creates_the_extension_the_models_sql_calls() {
    let Some(db) = pg_test_url::pg_test_db("postgresql_schema_extensions") else { return };
    let own = OwnDatabase::create(&db.url, &db.schema);
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
            exec(&own.url, &format!("CREATE SCHEMA {schema}"));
            let cleanup = DropOnExit { url: &own.url, sql: format!("DROP SCHEMA IF EXISTS {schema} CASCADE; DROP SCHEMA IF EXISTS {ext_schema} CASCADE") };
            let answers = if side == "rails" {
                let answers = rails_answers(&own.url, &search_path, &schema_rb, &schema);
                drop(cleanup);
                answers
            } else {
                let options = format!("-c search_path={search_path}");
                let run = app(Some(POSTGRESQL_DATABASE_YML), &schema_rb).run_ruby_env(
                    PROBE,
                    &[("DATABASE_URL", own.url.as_str()), ("PGOPTIONS", options.as_str()), ("RH_TEST_SCHEMA", schema.as_str())],
                );
                drop(cleanup);
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

/// `drop_schema` is `DROP SCHEMA ... CASCADE`. On ActiveRecord 8.1.4.1
/// that drops the extensions created in that schema, and what depends on
/// them in other schemas too: `public.earthdistance` when `cube` was in
/// the dropped schema, and a `public` table's `citext` column. The fold
/// can't tell what depends on an extension, so dropping a schema that
/// holds one is refused, like `disable_extension ..., force: :cascade`.
/// A schema that holds none of the recorded extensions still drops.
#[test]
fn drop_schema_with_an_extension_in_it_is_refused() {
    for body in [
        "    create_schema \"extensions\"\n    enable_extension \"extensions.cube\"\n    enable_extension \"public.earthdistance\"\n    drop_schema \"extensions\"",
        "    create_schema \"extensions\"\n    enable_extension \"extensions.pgcrypto\"\n    enable_extension \"cube\"\n    drop_schema \"extensions\"",
    ] {
        let error = folded_database_objects(body).expect_err("dropping a schema that holds an extension is refused");
        assert!(error.contains("drop_schema \"extensions\""), "{error}");
    }
    let ddl = folded_database_objects("    create_schema \"reports\"\n    create_schema \"extensions\"\n    enable_extension \"extensions.pgcrypto\"\n    drop_schema \"reports\"");
    assert_eq!(ddl, Ok(vec![r#"CREATE SCHEMA IF NOT EXISTS "extensions""#.to_string(), r#"CREATE EXTENSION IF NOT EXISTS "pgcrypto" SCHEMA extensions"#.to_string()]));
}

/// Rails writes an extension's schema unquoted (`SCHEMA EXTENSIONS`), so
/// PostgreSQL folds it to lowercase, while `create_schema` and
/// `drop_schema` quote theirs. On ActiveRecord 8.1.4.1 with PostgreSQL 18,
/// `enable_extension "EXTENSIONS.pgcrypto"` installs into `extensions`,
/// `enable_extension '"Ext".pgcrypto'` into `Ext`, and `drop_schema` of
/// either takes the extension.
#[test]
fn an_extension_schema_is_matched_as_postgresql_reads_it() {
    for (body, dropped) in [
        ("    create_schema \"extensions\"\n    enable_extension \"EXTENSIONS.pgcrypto\"\n    drop_schema \"extensions\"", "drop_schema \"extensions\""),
        ("    create_schema \"Ext\"\n    enable_extension '\"Ext\".pgcrypto'\n    drop_schema \"Ext\"", "drop_schema \"Ext\""),
    ] {
        let error = folded_database_objects(body).expect_err("the dropped schema holds the extension");
        assert!(error.contains(dropped), "{error}");
    }
}

/// An unqualified `enable_extension` installs into the first schema on the
/// search path. On ActiveRecord 8.1.4.1 with PostgreSQL 18, with the
/// default search path, `enable_extension "pgcrypto"`, `create_schema
/// "public", force: true`, `create_schema "extensions"`, `enable_extension
/// "extensions.pgcrypto"` leaves pgcrypto in `extensions` only. The fold
/// reads no search path, so it takes the extension to be in `public`, where
/// the default puts it unless a schema named after the user exists (#2260):
/// dropping `public` while an unqualified extension is recorded is refused,
/// and dropping another schema still folds.
#[test]
fn dropping_public_with_an_unqualified_extension_recorded_is_refused() {
    let error = folded_database_objects("    enable_extension \"pgcrypto\"\n    create_schema \"public\", force: true\n    create_schema \"extensions\"\n    enable_extension \"extensions.pgcrypto\"")
        .expect_err("public holds the unqualified pgcrypto");
    assert!(error.contains("create_schema \"public\" with force:") && error.contains("pgcrypto"), "{error}");
    let error = folded_database_objects("    enable_extension \"pgcrypto\"\n    drop_schema \"public\"").expect_err("public holds the unqualified pgcrypto");
    assert!(error.contains("drop_schema \"public\""), "{error}");
    let ddl = folded_database_objects("    create_schema \"reports\"\n    enable_extension \"pgcrypto\"\n    drop_schema \"reports\"");
    assert_eq!(ddl, Ok(vec![r#"CREATE EXTENSION IF NOT EXISTS "pgcrypto""#.to_string()]));
}

/// A name the fold can't read (`disable_extension target`) still runs on
/// Rails: on ActiveRecord 8.1.4.1, `enable_extension "pgcrypto"; target =
/// "pgcrypto"; disable_extension target` leaves no extension. Skipping it
/// would keep the recorded one, so a destructive call with such a name is
/// refused once there is something it could remove; with nothing recorded
/// it changes nothing the fold holds. A cascading one is refused whatever
/// is recorded, as a literal one is: after `target = "citext"; enable_extension
/// target` and a `t.citext :email` table, `disable_extension target, force:
/// :cascade` leaves `accounts` with only `id` on ActiveRecord 8.1.4.1 and
/// PostgreSQL 18.
#[test]
fn a_destructive_call_with_a_name_the_fold_cannot_read_is_refused() {
    let citext = "    target = \"citext\"\n    enable_extension target\n    create_table :accounts do |t|\n      t.citext :email\n    end\n";
    for (body, call) in [
        ("    enable_extension \"pgcrypto\"\n    target = \"pgcrypto\"\n    disable_extension target", "disable_extension"),
        (format!("{citext}    disable_extension target, force: :cascade").as_str(), "disable_extension"),
        (format!("{citext}    disable_extension target, **options").as_str(), "disable_extension"),
        ("    create_schema \"extensions\"\n    target = \"extensions\"\n    drop_schema target", "drop_schema"),
        ("    create_schema \"extensions\"\n    target = \"extensions\"\n    create_schema target, force: true", "create_schema"),
    ] {
        let error = folded_database_objects(body).expect_err("a destructive call with an unread name is refused");
        assert!(error.contains(call) && error.contains("name"), "{error}");
    }
    let ddl = folded_database_objects("    target = \"pgcrypto\"\n    disable_extension target\n    enable_extension \"citext\"");
    assert_eq!(ddl, Ok(vec![r#"CREATE EXTENSION IF NOT EXISTS "citext""#.to_string()]));
}

/// `rename_schema` is `ALTER SCHEMA ... RENAME TO` on ActiveRecord 8.1.4.1:
/// after `create_schema "old"; rename_schema "old", "extensions";
/// enable_extension "extensions.pgcrypto"`, PostgreSQL 18 has `extensions`
/// holding pgcrypto and no `old`. The fold doesn't model a rename, so the
/// migration is refused rather than folded into a boot that creates `old`
/// and then fails on the missing `extensions`.
#[test]
fn rename_schema_is_refused() {
    let error = folded_database_objects("    create_schema \"old\"\n    rename_schema \"old\", \"extensions\"\n    enable_extension \"extensions.pgcrypto\"")
        .expect_err("a rename the fold doesn't model");
    assert!(error.contains("rename_schema"), "{error}");
}

/// `create_schema "x", force: true` is `drop_schema "x", if_exists: true`
/// and then `CREATE SCHEMA` (ActiveRecord 8.1.4.1), so the extensions
/// created in it go too. The fold applies the drop first: a schema with
/// no recorded extension is recreated, and one that holds an extension
/// is refused, as `drop_schema` is.
#[test]
fn create_schema_with_force_drops_the_schema_first() {
    let ddl = folded_database_objects("    create_schema \"reports\"\n    create_schema \"reports\", force: true");
    assert_eq!(ddl, Ok(vec![r#"CREATE SCHEMA IF NOT EXISTS "reports""#.to_string()]));
    let error = folded_database_objects("    create_schema \"extensions\"\n    enable_extension \"extensions.pgcrypto\"\n    create_schema \"extensions\", force: true\n    enable_extension \"public.pgcrypto\"")
        .expect_err("recreating a schema that holds an extension is refused");
    assert!(error.contains("create_schema \"extensions\"") && error.contains("force:"), "{error}");
}

/// A keyword splat hides `force:` from the fold
/// (`disable_extension "cube", **{ force: :cascade }` is Rails' cascading
/// drop), so the fold refuses a splat it can't read through.
#[test]
fn a_keyword_splat_on_disable_extension_is_refused() {
    for splat in ["**{ force: :cascade }", "**options"] {
        let body = format!("    options = {{ force: :cascade }}\n    enable_extension \"cube\"\n    enable_extension \"earthdistance\"\n    disable_extension \"cube\", {splat}");
        let error = folded_database_objects(&body).expect_err("a keyword splat is refused");
        assert!(error.contains("disable_extension") && error.contains("**"), "{splat}: {error}");
    }
}

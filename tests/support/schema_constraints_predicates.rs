use super::*;

const SQLITE: &str = "test:\n  adapter: sqlite3\n  database: storage/test.sqlite3\n";
const POSTGRES: &str = "test:\n  adapter: postgresql\n  database: predicates_test\n";

// Rows are SQL literals for (code, payload, mode, parent_id, amount).
struct Predicate {
    name: &'static str,
    sql: &'static str,
    valid: &'static [&'static str],
    invalid: &'static [&'static str],
}

const PREDICATES: &[Predicate] = &[
    Predicate {
        name: "glob",
        sql: "code GLOB '[A-Z]*'",
        valid: &["'Alpha',NULL,NULL,NULL,0", "'Beta',NULL,NULL,NULL,1"],
        invalid: &["'alpha',NULL,NULL,NULL,0"],
    },
    Predicate {
        name: "not_glob",
        sql: "length(code) > 0 AND code NOT GLOB '*[^_a-zA-Z0-9]*'",
        valid: &["'valid_123',NULL,NULL,NULL,0", "'Safe_456',NULL,NULL,NULL,1"],
        invalid: &["'',NULL,NULL,NULL,0", "'has-dash',NULL,NULL,NULL,0", "'café',NULL,NULL,NULL,0"],
    },
    Predicate {
        name: "like",
        sql: "code LIKE 'a_%'",
        valid: &["'Ab',NULL,NULL,NULL,0", "'abc',NULL,NULL,NULL,0"],
        invalid: &["'a',NULL,NULL,NULL,0"],
    },
    Predicate {
        name: "not_like",
        sql: "code NOT LIKE 'x%'",
        valid: &["'alpha',NULL,NULL,NULL,0", "'beta',NULL,NULL,NULL,1"],
        invalid: &["'Xray',NULL,NULL,NULL,0"],
    },
    // The second column is named `like` here (see `schema`): the name is a
    // column where an operand stands and the operator after one.
    Predicate {
        name: "like_column",
        sql: "like IS NULL OR code LIKE like || '%' AND \"like\" NOT LIKE 'z%'",
        valid: &["'Abc',NULL,NULL,NULL,0", "'Abc','a',NULL,NULL,0"],
        invalid: &["'b','a',NULL,NULL,0", "'zeta','z',NULL,NULL,0"],
    },
    Predicate {
        name: "json_type",
        sql: "\"payload\" IS NULL OR json_valid(\"payload\") AND json_type(\"payload\") = 'array'",
        valid: &["'x',NULL,NULL,NULL,0", "'x','[]',NULL,NULL,0"],
        invalid: &["'x','{}',NULL,NULL,0", "'x','broken',NULL,NULL,0"],
    },
    Predicate {
        name: "simple_case",
        sql: "CASE mode WHEN 'linked' THEN parent_id IS NOT NULL WHEN 'detached' THEN parent_id IS NULL ELSE true END",
        valid: &["'x',NULL,'linked',1,0", "'x',NULL,'detached',NULL,0", "'x',NULL,NULL,NULL,0", "'x',NULL,'other',NULL,0"],
        invalid: &["'x',NULL,'linked',NULL,0", "'x',NULL,'detached',1,0"],
    },
    Predicate {
        name: "searched_case",
        sql: "CASE WHEN amount < 0 THEN code = 'negative' WHEN amount = 0 THEN parent_id IS NULL ELSE code = 'positive' END",
        valid: &["'negative',NULL,NULL,NULL,-1", "'x',NULL,NULL,NULL,0", "'positive',NULL,NULL,NULL,1"],
        invalid: &["'positive',NULL,NULL,NULL,-1", "'x',NULL,NULL,1,0", "'negative',NULL,NULL,NULL,1"],
    },
    Predicate {
        name: "between",
        sql: "amount BETWEEN 2 AND 4",
        valid: &["'x',NULL,NULL,NULL,2", "'x',NULL,NULL,NULL,4", "'x',NULL,NULL,NULL,NULL"],
        invalid: &["'x',NULL,NULL,NULL,1", "'x',NULL,NULL,NULL,5"],
    },
    Predicate {
        name: "not_between",
        sql: "amount NOT BETWEEN 2 AND 4",
        valid: &["'x',NULL,NULL,NULL,1", "'x',NULL,NULL,NULL,5", "'x',NULL,NULL,NULL,NULL"],
        invalid: &["'x',NULL,NULL,NULL,2", "'x',NULL,NULL,NULL,4"],
    },
    Predicate {
        name: "concat",
        sql: "code || mode = 'ab'",
        valid: &["'a',NULL,'b',NULL,0", "'a',NULL,NULL,NULL,0"],
        invalid: &["'a',NULL,'c',NULL,0"],
    },
    Predicate {
        name: "char",
        sql: "code = char(65, 66, 233)",
        valid: &["'ABé',NULL,NULL,NULL,0", "'ABé',NULL,NULL,NULL,1"],
        invalid: &["'AB',NULL,NULL,NULL,0"],
    },
    Predicate {
        name: "substr",
        sql: "substr(code, -2, 1) = 'b' AND substr(code, 2) = 'bc'",
        valid: &["'abc',NULL,NULL,NULL,0", "'zbc',NULL,NULL,NULL,1"],
        invalid: &["'acd',NULL,NULL,NULL,0"],
    },
    // AND binds tighter than OR.
    Predicate {
        name: "precedence",
        sql: "code = 'always' OR amount > 0 AND mode = 'ready'",
        valid: &["'always',NULL,'other',NULL,-1", "'x',NULL,'ready',NULL,1"],
        invalid: &["'x',NULL,'other',NULL,1", "'x',NULL,'ready',NULL,-1"],
    },
    Predicate {
        name: "combinations",
        sql: "CASE WHEN amount NOT BETWEEN 1 + 1 AND 3 * 2 THEN code || char(45) || substr(mode, 2, 2) NOT GLOB '*[^a-z-]*' ELSE code LIKE 'ok%' END",
        valid: &["'a',NULL,'xbc',NULL,1", "'ok',NULL,NULL,NULL,3"],
        invalid: &["'A',NULL,'xbc',NULL,1", "'bad',NULL,NULL,NULL,3"],
    },
    Predicate {
        name: "nested_case",
        sql: "CASE WHEN amount = 0 THEN CASE mode WHEN 'linked' THEN parent_id IS NOT NULL ELSE true END END",
        valid: &["'x',NULL,'linked',1,0", "'x',NULL,'linked',NULL,1"],
        invalid: &["'x',NULL,'linked',NULL,0"],
    },
];

/// Main's Portable grammar already reads these; the others are SQLite's own.
const SHARED: &[&str] = &["between", "not_between", "concat", "substr", "precedence"];
const USES_LIKE: &[&str] = &["like", "not_like", "like_column", "combinations"];

fn predicate(name: &str) -> &'static Predicate {
    PREDICATES.iter().find(|p| p.name == name).unwrap()
}

fn columns(predicate: &Predicate) -> &'static str {
    if predicate.name == "like_column" { "code,\"like\",mode,parent_id,amount" } else { "code,payload,mode,parent_id,amount" }
}

fn schema_with(predicate: &Predicate, declaration: &str) -> String {
    let second = if predicate.name == "like_column" { "like" } else { "payload" };
    format!(
        r#"ActiveRecord::Schema[8.1].define(version: 1) do
  create_table :predicate_items do |t|
    t.text :code, null: false
    t.text :{second}
    t.text :mode
    t.integer :parent_id
    t.integer :amount
    {declaration}
  end
end
"#
    )
}

fn schema(predicate: &Predicate) -> String {
    schema_with(predicate, &format!("t.check_constraint {:?}", predicate.sql))
}

/// The schema as application ingest reads it beside this `config/database.yml`.
fn app_schema(schema: &str, database: Option<&str>) -> Result<roundhouse::schema::Schema, String> {
    let mut tree = std::collections::HashMap::new();
    tree.insert(std::path::PathBuf::from("db/schema.rb"), schema.as_bytes().to_vec());
    if let Some(database) = database {
        tree.insert(std::path::PathBuf::from("config/database.yml"), database.as_bytes().to_vec());
    }
    roundhouse::ingest::ingest_app_from_tree(tree).map(|app| app.schema).map_err(|error| error.to_string())
}

fn sqlite_ddl(schema: &roundhouse::schema::Schema) -> String {
    render_schema_statements_for(schema, Dialect::Sqlite).unwrap().join(";\n")
}

fn contract(predicate: &Predicate) -> String {
    let columns = columns(predicate);
    let mut script = String::from(
        r#"
def reject_write(sql)
  begin
    execute(sql)
  rescue StandardError => error
    raise unless error.message.include?('CHECK constraint failed') || error.message.include?('malformed JSON')
    return
  end
  raise 'invalid write accepted: ' + sql
end
"#,
    );
    for values in predicate.valid {
        for statement in [
            format!("INSERT INTO predicate_items ({columns}) VALUES ({values})"),
            format!("UPDATE predicate_items SET ({columns}) = ({values})"),
        ] {
            script.push_str(&format!("execute({statement:?})\n"));
        }
    }
    let (last, count) = (predicate.valid.last().unwrap(), predicate.valid.len());
    for values in predicate.invalid {
        for statement in [
            format!("INSERT INTO predicate_items ({columns}) VALUES ({values})"),
            format!("UPDATE predicate_items SET ({columns}) = ({values})"),
        ] {
            script.push_str(&format!("reject_write({statement:?})\n"));
            let query = format!("SELECT COUNT(*) FROM predicate_items WHERE ({columns}) IS ({last})");
            script.push_str(&format!("raise 'rejected write changed rows' unless scalar({query:?}) == {count}\n"));
        }
    }
    script.push_str(&format!("puts {:?}\n", format!("{} inserts and updates passed", predicate.name)));
    script
}

fn assert_ruby(script: &str, expected: &str) {
    let out = emit_and_run::ruby().args(["-e", script]).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    if stdout.contains("skip:") {
        prerequisite::missing(stdout.trim());
        return;
    }
    assert!(stdout.contains(expected), "{stdout}");
}

/// A sqlite3 application's CHECK reaches SQLite DDL as written, and that DDL
/// accepts and rejects the rows `rails_enforces_all_predicate_inserts_and_updates`
/// shows Rails does.
fn sqlite_case(name: &str) {
    let predicate = predicate(name);
    let ddl = sqlite_ddl(&app_schema(&schema(predicate), Some(SQLITE)).unwrap());
    assert!(ddl.contains(&format!("CHECK ({})", predicate.sql)), "{ddl}");
    let script = format!(
        r#"
begin
  gem 'sqlite3', '>= 2.9'
rescue Gem::LoadError
  puts 'skip: sqlite3 unavailable'
  exit
end
require 'sqlite3'
DB = SQLite3::Database.new(':memory:')
DB.execute_batch({ddl:?})
def execute(sql) = DB.execute(sql)
def scalar(sql) = DB.get_first_value(sql)
{}
"#,
        contract(predicate)
    );
    assert_ruby(&script, &format!("{name} inserts and updates passed"));
}

macro_rules! predicate_tests {
    ($($name:ident),* $(,)?) => {$(#[test] fn $name() { sqlite_case(stringify!($name)); })*};
}
predicate_tests!(
    glob, not_glob, like, not_like, like_column, json_type, simple_case, searched_case, between, not_between,
    concat, char, substr, precedence, combinations, nested_case
);

#[test]
fn rails_enforces_all_predicate_inserts_and_updates() {
    let helper = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/rails_oracle.rb");
    let mut script = format!(
        r#"
require {helper:?}
unless RailsOracle.available?('railties', 'activerecord', sqlite3: '>= 2.9')
  puts 'skip: Rails or sqlite3 unavailable'
  exit
end
RailsOracle.activate('railties', 'activerecord', sqlite3: '>= 2.9')
require 'rails'
require 'active_record/railtie'
class OracleApp < Rails::Application
  config.load_defaults 8.1
  config.eager_load = false
  config.secret_key_base = 'test'
  config.logger = Logger.new(File::NULL)
end
ENV['DATABASE_URL'] = 'sqlite3::memory:'
OracleApp.initialize!
ActiveRecord::Base.establish_connection(adapter: 'sqlite3', database: ':memory:')
ActiveRecord::Schema.verbose = false
def execute(sql) = ActiveRecord::Base.connection.execute(sql)
def scalar(sql) = ActiveRecord::Base.connection.select_value(sql)
"#
    );
    for predicate in PREDICATES {
        script.push_str(&schema(predicate).replace("create_table :predicate_items do", "create_table :predicate_items, force: true do"));
        script.push_str(&contract(predicate));
    }
    script.push_str("puts \"oracle Ruby=#{RUBY_VERSION} Rails=#{Rails.version} defaults=8.1 SQLite=#{SQLite3::SQLITE_VERSION}\"\n");
    assert_ruby(&script, "nested_case inserts and updates passed");
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn native_spinel_enforces_all_predicate_inserts_and_updates() {
    // Each case has its own table so the native contract exercises every predicate.
    let mut source = String::new();
    let mut script = String::from(
        "def execute(sql)\n Db.exec(sql)\nend\ndef scalar(sql)\n stmt = Db.prepare(sql)\n raise 'no row' unless Db.step?(stmt)\n value = Db.column_int(stmt, 0)\n Db.finalize(stmt)\n value\nend\n",
    );
    for predicate in PREDICATES {
        let table = format!("predicate_{}", predicate.name);
        source.push_str(&schema(predicate).replace("predicate_items", &table));
        script.push_str(&contract(predicate).replace("predicate_items", &table));
    }
    emit_and_run::empty_app()
        .write("db/schema.rb", &source)
        .write("config/database.yml", SQLITE)
        .run_spinel_with_db(&script)
        .assert_passes();
}

/// The three reductions carry no `config/database.yml`; none uses `LIKE`.
#[test]
fn reductions_emit_strictly_without_a_database_configuration() {
    for name in ["not_glob", "json_type", "simple_case"] {
        let predicate = predicate(name);
        for target in [roundhouse::project::BuildTarget::Ruby, roundhouse::project::BuildTarget::Spinel] {
            let (tree, errors) = emit_and_run::empty_app().write("db/schema.rb", &schema(predicate)).emit(target);
            assert!(errors.is_empty(), "{name}: {errors:?}");
            let ddl = std::fs::read_to_string(tree.join("config/schema.rb")).unwrap();
            assert!(ddl.contains(predicate.sql) || ddl.contains(&predicate.sql.replace('"', "\\\"")), "{ddl}");
        }
    }
}

#[test]
fn every_sqlite_target_carries_the_checks_as_sql() {
    use roundhouse::project::BuildTarget;
    let source = ["not_glob", "json_type", "simple_case", "like"]
        .iter()
        .map(|name| schema(predicate(name)).replace("predicate_items", &format!("predicate_{name}")))
        .collect::<Vec<_>>()
        .join("\n");
    for target in BuildTarget::ALL.iter().copied().chain([BuildTarget::Roda]) {
        if matches!(target, BuildTarget::Blog | BuildTarget::Futamura) {
            continue;
        }
        let (tree, errors) = emit_and_run::empty_app()
            .write("db/schema.rb", &source)
            .write("config/database.yml", SQLITE)
            .emit(target);
        assert!(errors.is_empty(), "{}: {errors:?}", target.as_str());
        fn contents(path: &std::path::Path, text: &mut String) {
            for entry in std::fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    contents(&path, text);
                } else if let Ok(value) = std::fs::read_to_string(path) {
                    text.push_str(&value);
                }
            }
        }
        let mut text = String::new();
        contents(&tree, &mut text);
        for sql in ["NOT GLOB '*[^_a-zA-Z0-9]*'", "json_type(", "CASE mode WHEN 'linked' THEN", "code LIKE 'a_%'"] {
            assert!(text.contains(sql), "{} lost {sql}", target.as_str());
        }
    }
}

#[test]
fn roda_sqlite_migrations_enforce_the_same_predicates() {
    let mut script = String::from(
        r#"
begin
  gem 'sequel', '>= 5.0'
  gem 'sqlite3', '>= 2.9'
rescue Gem::LoadError
  puts 'skip: Sequel or sqlite3 unavailable'
  exit
end
require 'sequel'
require 'sequel/extensions/migration'
DB = Sequel.sqlite
def execute(sql) = DB.run(sql)
def scalar(sql) = DB.fetch(sql).first.values.first
"#,
    );
    for predicate in PREDICATES {
        let mut app = roundhouse::App::default();
        app.schema = app_schema(&schema(predicate), Some(SQLITE)).unwrap();
        let files = roundhouse::emit::roda::emit(&app);
        let migration = files.iter().find(|f| f.path.to_string_lossy().contains("create_predicate_items")).unwrap();
        script.push_str(&format!("DB.drop_table?(:predicate_items)\neval({:?}).apply(DB, :up)\n", migration.content));
        script.push_str(&contract(predicate));
    }
    assert_ruby(&script, "nested_case inserts and updates passed");
}

#[test]
fn migrations_read_sqlite_checks_as_the_schema_does() {
    let migration = "class CreateItems < ActiveRecord::Migration[8.1]\n  def change\n    create_table :items do |t|\n      t.text :code\n      t.check_constraint \"code GLOB '[a-z]*'\"\n    end\n    add_check_constraint :items, \"CASE WHEN code LIKE 'a%' THEN length(code) > 1 ELSE true END\", name: 'items_case'\n  end\nend\n";
    let app = |database: Option<&str>| {
        let mut tree = std::collections::HashMap::new();
        tree.insert(std::path::PathBuf::from("db/migrate/20260101000000_create_items.rb"), migration.as_bytes().to_vec());
        if let Some(database) = database {
            tree.insert(std::path::PathBuf::from("config/database.yml"), database.as_bytes().to_vec());
        }
        roundhouse::ingest::ingest_app_from_tree(tree).map(|app| sqlite_ddl(&app.schema)).map_err(|e| e.to_string())
    };
    let ddl = app(Some(SQLITE)).unwrap();
    assert!(ddl.contains("CHECK (code GLOB '[a-z]*')") && ddl.contains("CHECK (CASE WHEN code LIKE 'a%' THEN length(code) > 1 ELSE true END)"), "{ddl}");
    for database in [None, Some(POSTGRES)] {
        assert!(app(database).unwrap_err().contains("unsupported"), "{database:?}");
    }
}

#[test]
fn sqlite_checks_read_spellings_and_keep_the_gate_narrow() {
    let base = predicate("glob");
    let with = |sql: &str| app_schema(&schema_with(base, &format!("t.check_constraint {sql:?}")), Some(SQLITE));
    for sql in [
        "CASE(mode) WHEN('linked') THEN(parent_id IS NOT NULL) ELSE(1) END",
        "case when (amount between 1 and 3) then (CHAR (65) || SUBSTR (code, 1)) not glob 'x*' else true end",
        "JSON_TYPE (payload, '$') IS NULL OR code NOT LIKE ('x' || '%')",
        "code = 'it''s CASE WHEN GLOB LIKE char(0); -- literal data'",
        "code\nGLOB\r\n\t'a*' AND amount IN (1, -2, +3) AND NOT amount == 4 AND amount <> -parent_id",
        "CASE WHEN code GLOB 'a*' THEN NULL END IS NOT NULL OR char() = ''",
    ] {
        let ddl = sqlite_ddl(&with(sql).unwrap_or_else(|error| panic!("{sql}: {error}")));
        assert!(ddl.contains(&format!("CHECK ({sql})")), "{ddl}");
    }
    for sql in [
        "CASE WHEN amount > 0 THEN unknown(code) ELSE true END",
        "CASE missing WHEN 1 THEN true ELSE false END",
        "code GLOB unknown_pattern",
        "json_type(missing) = 'array'",
        "code REGEXP 'x'",
        "code MATCH 'x'",
        "code LIKE 'x' ESCAPE '!'",
        "code LIKE 'x'; SELECT 1",
        "code GLOB /* unknown function */ mystery(code)",
        "code GLOB 'a*' -- AND amount > 0",
        "\"char\"(65) = code",
        "\"missing\" GLOB 'a*'",
        "code/**/(65) IS NULL",
        "glob(code, 'a*')",
        "like('a%', code)",
        "code GLOB 'a*' COLLATE nocase",
        "CAST(code AS integer) > 0 OR code GLOB 'a*'",
        "code GLOB 'a*' AND payload ->> 'kind' = 'x'",
        "code GLOB 'a*' AND amount << 1 > 0",
        "code GLOB 'a*' AND amount & 1 = 0",
        "code GLOB 'a*' AND code = x'41'",
        "code GLOB 'a*' AND amount = 1e2",
        "code GLOB 'a*' AND amount = 0x10",
        "code GLOB 'a*' AND items.amount > 0",
        "code GLOB 'a*' AND code = $$a$$",
        "code\u{a0}GLOB 'a*'",
        "cod\u{e9} GLOB 'a*'",
        "code GLOB 'a*' AND amount NOT NULL",
        "code GLOB 'a*' AND (SELECT 1)",
        "code GLOB 'a*' AND length(code) > CURRENT_TIMESTAMP",
        "code GLOB 'unterminated",
    ] {
        let error = with(sql).map(|_| ()).unwrap_err();
        assert!(error.contains("db/schema.rb") && error.contains("unsupported schema expression"), "{sql}: {error}");
    }
}

/// `like`, `glob` and `end` name columns wherever an operand stands, and
/// the operator or keyword after one: the list of columns decides nothing.
#[test]
fn words_are_operators_or_names_by_position() {
    let table = |declaration: &str| {
        format!("ActiveRecord::Schema[8.1].define do\n create_table :items do |t|\n  t.text :code\n  t.text :like\n  t.text :glob\n  t.integer :end\n  {declaration}\n end\nend\n")
    };
    let check = |sql: &str, database: Option<&str>| app_schema(&table(&format!("t.check_constraint {sql:?}")), database);
    for sql in [
        "like LIKE glob OR glob GLOB like",
        "CASE like WHEN glob THEN end ELSE end + 1 END > 0",
        "(like) NOT LIKE (glob) AND \"end\" BETWEEN end AND end",
    ] {
        let ddl = sqlite_ddl(&check(sql, Some(SQLITE)).unwrap_or_else(|error| panic!("{sql}: {error}")));
        assert!(ddl.contains(&format!("CHECK ({sql})")), "{ddl}");
    }
    // Without a configuration the operator stays refused beside the column;
    // the column alone, and GLOB, read as written.
    for sql in ["CASE WHEN code LIKE 'a%' THEN true ELSE false END", "char(65) = code AND code LIKE like", "json_type(code) IS NULL OR code NOT LIKE glob"] {
        assert!(check(sql, None).is_err(), "{sql}");
    }
    let sql = "like IS NULL OR glob GLOB 'a*' AND CASE like WHEN glob THEN end END > 0";
    assert!(sqlite_ddl(&check(sql, None).unwrap()).contains(&format!("CHECK ({sql})")));
    // An operand cannot follow an operand, nor an operator an operator.
    let plain = |sql: &str| app_schema(&schema_with(predicate("glob"), &format!("t.check_constraint {sql:?}")), Some(SQLITE));
    for sql in ["code GLOB GLOB 'a'", "code glob 'a' 'b'", "code GLOB 'a*' = * 1", "GLOB 'a*'", "WHEN code GLOB 'a*'", "code GLOB 'a*' (mode)", "mode code GLOB 'a*'", "code GLOB", "code NOT GLOB 'a*' AND"] {
        assert!(plain(sql).is_err(), "{sql}");
    }
}

/// With no configuration the database is not known: `LIKE`, which PostgreSQL
/// reads with case, stays refused; what PostgreSQL rejects or reads alike emits.
#[test]
fn an_unconfigured_application_keeps_like_refused() {
    // `like_column` is left out: main's gate already reads it, taking the operator for the column.
    for predicate in PREDICATES.iter().filter(|p| p.name != "like_column") {
        let read = app_schema(&schema(predicate), None);
        if USES_LIKE.contains(&predicate.name) {
            assert!(read.unwrap_err().contains("unsupported schema expression"), "{}", predicate.name);
        } else {
            let ddl = sqlite_ddl(&read.unwrap_or_else(|error| panic!("{}: {error}", predicate.name)));
            assert!(ddl.contains(&format!("CHECK ({})", predicate.sql)), "{ddl}");
        }
    }
}

/// Another database's CHECK is read as on main: only the shared grammar, so a
/// PostgreSQL expression SQLite would read differently never reaches SQLite.
#[test]
fn another_database_keeps_the_portable_grammar() {
    let mysql = "test:\n  adapter: mysql2\n  database: predicates_test\n";
    let computed = "<% if ENV['PG'] %>\ntest:\n  adapter: postgresql\n<% else %>\ntest:\n  adapter: sqlite3\n<% end %>\n";
    let production = "production:\n  adapter: postgresql\n  database: predicates\n";
    for database in [POSTGRES, mysql, computed, production, "test:\n  url: postgres://localhost/predicates\n", ""] {
        for predicate in PREDICATES.iter().filter(|p| p.name != "like_column") {
            let read = app_schema(&schema(predicate), Some(database));
            assert_eq!(read.is_ok(), SHARED.contains(&predicate.name), "{} under {database:?}", predicate.name);
        }
    }
    // The parked candidate emitted this for SQLite: PostgreSQL rejects `ABC` and SQLite accepts it.
    let like_column = "ActiveRecord::Schema[8.1].define do\n create_table :predicate_items do |t|\n  t.text :code\n  t.text :like\n  t.check_constraint \"CASE WHEN code LIKE 'a%' THEN true ELSE false END\"\n end\nend\n";
    for database in [Some(POSTGRES), None] {
        let error = app_schema(like_column, database).map(|_| ()).unwrap_err();
        assert!(error.contains("unsupported schema expression on predicate_items: CASE WHEN code LIKE"), "{error}");
    }
}

/// The environment the emitted tests run in decides, as it does the dialect.
#[test]
fn the_test_environment_names_the_database() {
    let like = schema(predicate("like"));
    let sqlite_tests = "production:\n  adapter: postgresql\n  database: predicates\ntest:\n  adapter: sqlite3\n  database: storage/test.sqlite3\n";
    let postgres_tests = "production:\n  adapter: sqlite3\n  database: storage/production.sqlite3\ntest:\n  adapter: postgresql\n  database: predicates_test\n";
    assert!(app_schema(&like, Some(sqlite_tests)).is_ok());
    assert!(app_schema(&like, Some("production:\n  adapter: sqlite3\n  database: storage/production.sqlite3\n")).is_ok());
    assert!(app_schema(&like, Some(postgres_tests)).is_err());
}

/// The partial-index predicates of the review history, in a PostgreSQL
/// application emitted for PostgreSQL: each renders as written, as on main.
#[test]
fn postgres_partial_index_predicates_render_as_on_main() {
    for sql in [
        "CASE WHEN mode = 'active' THEN true ELSE false END",
        "mode LIKE 'active%'",
        "mode <> $label$CASE$label$",
        r"mode <> E'GLOB \' escaped'",
        "mode <> $$CASE GLOB json_type('[])' LIKE$$",
        "code$tag$ IS NOT NULL",
        "mode <> $é$json_type(1)$é$",
        "mode IS NOT NULL /* outer /* inner */ code GLOB json_type(1) */",
        "mode GLOB 'a*'",
        "json_type(mode) = 'array'",
    ] {
        let source = format!("ActiveRecord::Schema[8.1].define do\n  create_table :items do |t|\n    t.text :code\n    t.text :mode\n    t.integer \"code$tag$\"\n    t.index :code, unique: true, name: 'items_code_partial', where: {sql:?}\n  end\nend\n");
        let (_tree, errors) = emit_and_run::empty_app()
            .write("db/schema.rb", &source)
            .write("config/database.yml", POSTGRES)
            .emit(roundhouse::project::BuildTarget::Ruby);
        assert!(errors.is_empty(), "{sql}: {errors:?}");
        let ddl = render_schema_statements_for(&app_schema(&source, Some(POSTGRES)).unwrap(), Dialect::Postgres).unwrap().join(";\n");
        assert!(ddl.contains(&format!("WHERE {sql}")), "{sql}: {ddl}");
    }
}

/// What SQLite DDL carries as SQLite's own text has no PostgreSQL rendering;
/// the shared grammar still renders for both, and the source survives the IR.
#[test]
fn sqlite_text_reaches_sqlite_ddl_alone() {
    for name in ["glob", "like", "simple_case", "json_type", "char"] {
        let schema = app_schema(&schema(predicate(name)), Some(SQLITE)).unwrap();
        let error = render_schema_statements_for(&schema, Dialect::Postgres).unwrap_err();
        assert!(error.contains("unsupported schema expression"), "{name}: {error}");
        let stored: roundhouse::schema::Schema = serde_json::from_str(&serde_json::to_string(&schema).unwrap()).unwrap();
        assert_eq!(stored, schema);
        assert_eq!(sqlite_ddl(&stored), sqlite_ddl(&schema));
        // The same tables without their source are refused, as before.
        let unsourced = roundhouse::schema::Schema { tables: schema.tables.clone(), ..Default::default() };
        assert!(render_schema_statements_for(&unsourced, Dialect::Sqlite).is_err(), "{name}");
    }
    let shared = app_schema(&schema(predicate("between")), Some(SQLITE)).unwrap();
    for dialect in [Dialect::Sqlite, Dialect::Postgres] {
        assert!(render_schema_statements_for(&shared, dialect).unwrap().join(";\n").contains("CHECK (amount BETWEEN 2 AND 4)"));
    }
}

/// Only a CHECK reads SQLite's own grammar: indexes, their predicates and
/// defaults keep main's, also where a CHECK spells the same text.
#[test]
fn indexes_and_defaults_keep_the_portable_grammar() {
    let base = predicate("glob");
    for declaration in [
        "t.index \"code GLOB 'a*'\", name: 'items_glob'",
        "t.index \"CASE WHEN code GLOB 'a*' THEN code END\", name: 'items_case'",
        "t.index 'lower(code)', unique: true, name: 'items_lower', where: \"mode GLOB 'a*'\"",
        "t.text :label, default: -> { \"char(65)\" }",
        "t.check_constraint \"code GLOB 'a*'\"\n    t.index \"code GLOB 'a*'\", name: 'items_glob'",
    ] {
        for database in [Some(SQLITE), None] {
            let error = app_schema(&schema_with(base, declaration), database).map(|_| ()).unwrap_err();
            assert!(error.contains("unsupported"), "{declaration}: {error}");
        }
    }
}

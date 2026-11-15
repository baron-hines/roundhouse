//! The Spinel `Db` shim over spinel-pg (runtime/spinel/db_pg.rb).
//!
//! The gate compiles the Db contract suite (tests/db_contract/) next to
//! the shim with Spinel, seeded with the shim's RBS, and runs it against
//! a live PostgreSQL: the contract cases every shim passes, the cases
//! any PostgreSQL Db passes, and db_pg.rb's own (its lane file,
//! tests/db_contract/db_pg_lane.rb). The suite drives the `Db` surface
//! directly; it and its lanes are described in tests/support/db_contract.rs.
//!
//! An emitted app whose primary database is PostgreSQL ships the shim as
//! its runtime/db.rb (`project::apply_postgresql_tree`, emission checked
//! in tests/postgresql_spinel_target.rs). The second gate compiles one of
//! that app's tests with Spinel and runs it on the server.
//!
//!   SPINEL_PG_DIR=/path/to/spinel-pg \
//!   ROUNDHOUSE_PG_TEST_URL=postgres://postgres:secret@127.0.0.1:5432/postgres \
//!   cargo test --test spinel_pg_db -- --ignored --nocapture
//!
//! Without ROUNDHOUSE_PG_TEST_URL the gate says so and skips, unless
//! ROUNDHOUSE_PG_TEST_REQUIRED is set, as CI sets it; a missing
//! SPINEL_PG_DIR fails it. Each run gets a schema of its own
//! (tests/support/pg_test_url.rs), dropped when it finishes.

#[path = "support/db_contract.rs"]
mod db_contract;
#[path = "support/emit_and_run.rs"]
mod emit_and_run;
#[path = "support/array_widgets.rs"]
mod array_widgets;

use db_contract::{Backend, Runtime, lanes, pg_test_url};
use roundhouse::project::BuildTarget;
use std::collections::BTreeSet;
use std::process::Command;

#[test]
#[ignore = "requires Spinel, SPINEL_PG_DIR (a spinel-pg checkout) and ROUNDHOUSE_PG_TEST_URL (PostgreSQL)"]
fn spinel_pg_shims_pass_the_contract_and_postgres_cases() {
    let mut ran = 0;
    for lane in lanes(Runtime::Spinel, Backend::Postgres) {
        db_contract::run_lane(lane);
        ran += 1;
    }
    assert!(ran > 0, "no Spinel PostgreSQL lane");
}

const POSTGRESQL_DATABASE_YML: &str = "default: &default\n  adapter: postgresql\n  encoding: unicode\n\ndevelopment:\n  <<: *default\n  database: blog_development\n\ntest:\n  <<: *default\n  database: blog_test\n";

/// The blog on PostgreSQL, with a boolean column a lowered `where`
/// compares with a literal, tables with no primary key, and tests of its
/// own: that literal is PostgreSQL's `TRUE` (SQLite's `1` is an error
/// against a boolean); rows go into keyless tables (which have no sequence
/// to read a key from, nor a `rowid`), one of them reading back a stored
/// generated column and another a database default; and the test runs in
/// the schema its helper made for this process.
fn postgresql_blog() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("config/database.yml", POSTGRESQL_DATABASE_YML)
        .edit(
            "db/schema.rb",
            "    t.text \"body\"\n    t.datetime \"created_at\", null: false\n    t.datetime \"updated_at\", null: false\n  end\n\n  create_table \"comments\"",
            "    t.text \"body\"\n    t.boolean \"published\", default: false, null: false\n    t.datetime \"created_at\", null: false\n    t.datetime \"updated_at\", null: false\n  end\n\n  create_table \"stars\", id: false, force: :cascade do |t|\n    t.bigint \"article_id\", null: false\n  end\n\n  create_table \"badges\", id: false, force: :cascade do |t|\n    t.string \"first_name\", null: false\n    t.virtual \"label\", type: :string, as: \"first_name || '!'\", stored: true\n  end\n\n  create_table \"visits\", id: false, force: :cascade do |t|\n    t.string \"name\"\n    t.datetime \"recorded_at\", default: -> { \"CURRENT_TIMESTAMP\" }, null: false\n  end\n\n  create_table \"comments\"",
        )
        .write("app/models/star.rb", "class Star < ApplicationRecord\nend\n")
        .write("app/models/badge.rb", "class Badge < ApplicationRecord\nend\n")
        .write("app/models/visit.rb", "class Visit < ApplicationRecord\nend\n")
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  def self.published_count\n    where(published: true).count\n  end\n\n",
        )
        .edit(
            "test/models/article_test.rb",
            "class ArticleTest < ActiveSupport::TestCase\n",
            "class ArticleTest < ActiveSupport::TestCase\n  test \"counts published articles\" do\n    assert_equal 0, Article.published_count\n    articles(:one).update!(published: true)\n    assert_equal 1, Article.published_count\n  end\n\n  test \"stars an article in a table with no key\" do\n    Star.create!(article_id: articles(:one).id)\n    assert_equal 1, Star.where(article_id: articles(:one).id).count\n  end\n\n  test \"reads back a keyless table's generated column\" do\n    badge = Badge.create!(first_name: \"hello\")\n    assert_equal \"hello!\", badge.label\n    assert_equal 1, Badge.count\n  end\n\n  test \"reads back a keyless table's database default\" do\n    visit = Visit.create!(name: \"seen\")\n    assert_not_nil visit.recorded_at\n    assert_equal 1, Visit.count\n  end\n\n  test \"creates a comment after the fixtures\" do\n    comment = Comment.create!(article: articles(:one), commenter: \"Carol\", body: \"A new comment\")\n    assert_equal 3, Comment.count\n    assert_equal \"Carol\", Comment.find(comment.id).commenter\n  end\n\n  test \"runs in a schema of its own\" do\n    schema = Article.connection.select_all(\"SELECT current_schema() AS name\").to_a.first[\"name\"]\n    assert schema.start_with?(\"rh_test_\"), schema\n  end\n\n",
        )
}

/// The emitted blog's model test, compiled with Spinel against spinel-pg
/// and run on a server: its helper makes the process a schema of its own
/// (Db.configure's `schema_search_path:`, set on each connection the pool
/// opens) and loads the boot DDL and the fixtures there; the tests
/// read, insert, validate, update and destroy through the PostgreSQL SQL
/// the emit lowered; the schema is dropped at exit.
#[test]
#[ignore = "requires Spinel, SPINEL_PG_DIR (a spinel-pg checkout) and ROUNDHOUSE_PG_TEST_URL (PostgreSQL)"]
fn an_emitted_apps_test_runs_natively_on_postgresql() {
    let Some(db) = pg_test_url::pg_test_db("spinel_pg_emitted_app") else { return };
    if std::env::var_os("RH_SPINEL_BUNDLE_COLLECT").is_some() {
        panic!("spinel bundle collect: not bundled");
    }
    let include = match std::env::var("SPINEL_PG_DIR") {
        Ok(dir) if !dir.is_empty() => dir,
        _ => panic!("SPINEL_PG_DIR must name a spinel-pg checkout"),
    };
    let (emitted, errors) = postgresql_blog().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    // As the emitted Makefile's spinel-test recipe compiles a test, with
    // spinel-pg found where `spin` would put its include path.
    let compiler = std::env::var("SPINEL").unwrap_or_else(|_| "spinel".into());
    let compiled = Command::new(&compiler)
        .args(["-I", &include, "-O", "0", "--no-inline-hot", "test/article_test.rb", "-o", "article_test"])
        .current_dir(&*emitted)
        .output()
        .expect("spawn spinel");
    assert!(
        compiled.status.success(),
        "spinel test/article_test.rb in {}:\n{}{}",
        emitted.display(),
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
    let stdout = db_contract::run_bounded(
        Command::new(emitted.join("article_test"))
            .env("DATABASE_URL", &db.url)
            .env_remove("PGOPTIONS")
            .env_remove("DATABASE_POOL_SIZE")
            .env_remove("RH_SQL_TRACE")
            .current_dir(&*emitted),
        db_contract::LANE_DEADLINE,
        "Spinel did not leave the compiled test behind",
    );
    assert!(stdout.contains("ArticleTest: 10 tests passed"), "{stdout}");
}

const WIDGET_FIXTURES: &str = "one:\n  name: one\n  tag_list: [\"a\", \"b,c\", \"it's\"]\n  scores: [1, 2]\n\ntwo:\n  name: two\n";

/// The array column cases of tests/postgresql_array_columns.rs, as one
/// model test the Spinel target compiles.
const WIDGET_TEST: &str = r##"require "test_helper"

class WidgetTest < ActiveSupport::TestCase
  # Fixtures on PostgreSQL leave the id sequence behind (#2164).
  setup do
    Widget.connection.select_all("SELECT setval(pg_get_serial_sequence('widgets', 'id'), 1000)")
  end

  test "boot made the GIN index" do
    index = Widget.connection.select_all("SELECT indexdef FROM pg_indexes WHERE indexname = 'index_widgets_on_tag_list'").to_a.first["indexdef"]
    assert index.include?("USING gin (tag_list)"), index
  end

  test "a fixture's sequence loads as the Array" do
    assert_equal ["a", "b,c", "it's"], widgets(:one).tag_list
    assert_equal [1, 2], widgets(:one).scores
    assert_nil widgets(:one).notes
    assert_equal [], widgets(:two).tag_list
    assert_equal [0], widgets(:two).scores
  end

  test "a new record reads the database's array defaults" do
    w = Widget.new(name: "new")
    assert_equal [], w.tag_list
    assert_equal [0], w.scores
    assert_nil w.notes
    w.save!
    f = Widget.find(w.id)
    assert_equal [], f.tag_list
    assert_equal [0], f.scores
    assert_nil f.notes
  end

  test "writes and loads through the model" do
    w = Widget.create!(name: "write")
    w.tag_list = ["a,b", "q\"t", "it's", "back\\slash", "", "NULL", " pad ", "{x}", "é"]
    w.scores = [3, -1]
    w.save!
    g = Widget.find(w.id)
    assert_equal ["a,b", "q\"t", "it's", "back\\slash", "", "NULL", " pad ", "{x}", "é"], g.tag_list
    assert_equal [3, -1], g.scores
    g.tag_list << "c"
    g.save!
    assert_equal "c", Widget.find(w.id).tag_list.last
  end

  test "nil and [] stay distinct" do
    w = Widget.create!(name: "notes")
    w.notes = []
    w.save!
    assert_equal [], Widget.find(w.id).notes
    w.notes = nil
    w.save!
    assert_nil Widget.find(w.id).notes
  end

  test "unnest, @> and pluck read the real array" do
    Widget.create!(name: "x", tag_list: ["x", "a,b"])
    assert_equal ["x"], Widget.tagged_by_unnest("a,b").pluck(:name)
    assert_equal 1, Widget.where("tag_list @> ARRAY[?]::text[]", "a,b").count
    assert_equal [["x", "a,b"]], Widget.where(name: "x").pluck(:tag_list)
    assert_equal [[0]], Widget.where(name: "x").pluck(:scores)
  end

  test "update_all and insert_all write the array literal" do
    w = Widget.create!(name: "bulk")
    Widget.retag(["x,y", "q\"t", "it's"])
    f = Widget.find(w.id)
    assert_equal ["x,y", "q\"t", "it's"], f.tag_list
    assert_equal [7, 8], f.scores
    Widget.bulk_insert([{ name: "inserted", tag_list: ["p", "{q}"], scores: [5] }])
    two = Widget.where(name: "inserted").first
    assert_equal ["p", "{q}"], two.tag_list
    assert_equal [5], two.scores
  end
end
"##;

/// PostgreSQL array columns (`text[]`, `integer[]`) under Spinel: the
/// PostgreSQL tree's codec (runtime/spinel/array_column_pg.rb, shipped
/// as runtime/array_column.rb) compiles with the columns typed
/// `Array[String]` / `Array[Integer]`, and the model test round-trips
/// through the server: fixtures, the `'{}'` / `'{0}'` defaults, writes
/// with quotes, commas, braces and backslashes, `unnest`, `@>` over the
/// GIN index, `pluck`, and `update_all` / `insert_all` through the
/// model's `encode_written`.
#[test]
#[ignore = "requires Spinel, SPINEL_PG_DIR (a spinel-pg checkout) and ROUNDHOUSE_PG_TEST_URL (PostgreSQL)"]
fn array_columns_round_trip_natively_on_postgresql() {
    let Some(db) = pg_test_url::pg_test_db("spinel_pg_array_columns") else { return };
    let include = match std::env::var("SPINEL_PG_DIR") {
        Ok(dir) if !dir.is_empty() => dir,
        _ => panic!("SPINEL_PG_DIR must name a spinel-pg checkout"),
    };
    // The blog, not the bare widgets app: the emitted test harness
    // dispatches requests through the blog's controllers, and Spinel
    // follows those `require_relative`s whether a test sends a request.
    let schema = array_widgets::SCHEMA;
    let start = schema.find("  create_table").expect("widgets table");
    let end = schema.rfind("end\n").expect("schema end");
    let (emitted, errors) = emit_and_run::real_blog()
        .write("config/database.yml", POSTGRESQL_DATABASE_YML)
        .edit("db/schema.rb", "  create_table \"comments\"", &format!("{}\n  create_table \"comments\"", &schema[start..end]))
        .write("app/models/widget.rb", array_widgets::WIDGET)
        .write("test/fixtures/widgets.yml", WIDGET_FIXTURES)
        .write("test/models/widget_test.rb", WIDGET_TEST)
        .emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let codec = std::fs::read_to_string(emitted.join("runtime/array_column.rb")).expect("runtime/array_column.rb");
    assert!(codec.contains("PostgreSQL's array input format"), "not the PostgreSQL codec:\n{codec}");
    let rbs = std::fs::read_to_string(emitted.join("app/models/widget.rbs")).expect("app/models/widget.rbs");
    for typed in [
        "  def tag_list: () -> Array[String]\n",
        "  def scores: () -> Array[Integer]?\n",
        "  def notes: () -> Array[String]?\n",
    ] {
        assert!(rbs.contains(typed), "{typed:?} not in app/models/widget.rbs:\n{rbs}");
    }
    let compiler = std::env::var("SPINEL").unwrap_or_else(|_| "spinel".into());
    let compiled = Command::new(&compiler)
        .args(["-I", &include, "-O", "0", "--no-inline-hot", "test/widget_test.rb", "-o", "widget_test"])
        .current_dir(&*emitted)
        .output()
        .expect("spawn spinel");
    assert!(
        compiled.status.success(),
        "spinel test/widget_test.rb in {}:\n{}{}",
        emitted.display(),
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
    let stdout = db_contract::run_bounded(
        Command::new(emitted.join("widget_test"))
            .env("DATABASE_URL", &db.url)
            .env_remove("PGOPTIONS")
            .env_remove("DATABASE_POOL_SIZE")
            .env_remove("RH_SQL_TRACE")
            .current_dir(&*emitted),
        db_contract::LANE_DEADLINE,
        "Spinel did not leave the compiled test behind",
    );
    assert!(stdout.contains("WidgetTest: 7 tests passed"), "{stdout}");
}

/// `(owner, name)` for every `def` in a Ruby or RBS source, with `self.`
/// kept on singleton methods. Owners are the top-level `class`/`module`
/// lines, which is all these two files use.
fn definitions(src: &str, rbs: bool) -> BTreeSet<(String, String)> {
    let mut owner = String::new();
    let mut out = BTreeSet::new();
    for line in src.lines() {
        for keyword in ["class ", "module "] {
            if let Some(rest) = line.strip_prefix(keyword) {
                owner = rest.split_whitespace().next().unwrap_or("").to_string();
            }
        }
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("def ") else {
            continue;
        };
        let end = rest
            .find(|c: char| {
                if rbs {
                    c == ':'
                } else {
                    c == '(' || c.is_whitespace()
                }
            })
            .unwrap_or(rest.len());
        out.insert((owner.clone(), rest[..end].trim().to_string()));
    }
    out
}

/// Every method the shim (db_pg.rb, pg_errors.rb, pg_driver.rb) defines
/// carries a signature: the contract in runtime/ruby/db.rbs, or the
/// shim's own db_pg.rbs / pg_errors.rbs / pg_driver.rbs. All of them
/// parse as RBS.
#[test]
fn db_pg_rbs_declares_every_method() {
    let read =
        |path: &str| std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let contract = read("runtime/ruby/db.rbs");
    let mut ruby = String::new();
    let mut own = String::new();
    for stem in [
        "runtime/spinel/db_pg",
        "runtime/spinel/pg_errors",
        "runtime/spinel/pg_driver",
    ] {
        ruby.push_str(&read(&format!("{stem}.rb")));
        ruby.push('\n');
        let rbs = read(&format!("{stem}.rbs"));
        roundhouse::rbs::parse_app_signatures(&rbs)
            .unwrap_or_else(|e| panic!("{stem}.rbs does not parse: {e}"));
        own.push_str(&rbs);
        own.push('\n');
    }
    roundhouse::rbs::parse_app_signatures(&contract)
        .unwrap_or_else(|e| panic!("runtime/ruby/db.rbs does not parse: {e}"));
    let mut declared = definitions(&own, true);
    declared.extend(definitions(&contract, true));
    let defined = definitions(&ruby, false);
    assert!(
        defined.len() > 60,
        "found only {} definitions",
        defined.len()
    );
    let missing: Vec<String> = defined
        .difference(&declared)
        .map(|(owner, name)| format!("{owner}#{name}"))
        .collect();
    assert!(
        missing.is_empty(),
        "the PostgreSQL shim defines methods with no RBS signature:\n  {}",
        missing.join("\n  ")
    );
    let stale: Vec<String> = definitions(&own, true)
        .difference(&defined)
        .map(|(owner, name)| format!("{owner}#{name}"))
        .collect();
    assert!(
        stale.is_empty(),
        "the PostgreSQL shim RBS declares methods the shim does not define:\n  {}",
        stale.join("\n  ")
    );
}

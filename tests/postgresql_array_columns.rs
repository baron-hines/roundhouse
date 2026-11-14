//! PostgreSQL array columns stored as real arrays (#91). On a PostgreSQL
//! database a stored array column (`lower::array_columns`) is an array of
//! its element, `text[]` or `integer[]`, so `unnest`, `@>` and a GIN index
//! work on it as they do under Rails. Its default is the database's array
//! literal (`'{}'`, `'{0}'`), and the tree's `ArrayColumn` reads and
//! writes the array formats (runtime/spinel/array_column_pg.rb). SQLite
//! keeps the Array as JSON text.
//!
//! The ingest, DDL and tree checks need no server. The codec and the app
//! runs do: they read ROUNDHOUSE_PG_TEST_URL through
//! tests/support/pg_test_url.rs, skipping without it unless
//! ROUNDHOUSE_PG_TEST_REQUIRED is set, and need the pg gem.
//!
//!   ROUNDHOUSE_PG_TEST_URL=postgres://postgres@127.0.0.1:5432/postgres \
//!   cargo test --test postgresql_array_columns

#[path = "support/emit_and_run.rs"]
mod emit_and_run;
#[path = "support/pg_test_url.rs"]
mod pg_test_url;

use std::collections::HashMap;
use std::path::PathBuf;

use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::project::BuildTarget;

const SCHEMA: &str = "ActiveRecord::Schema[8.1].define(version: 1) do
  create_table \"widgets\", force: :cascade do |t|
    t.string \"name\"
    t.text \"tag_list\", array: true, default: [], null: false
    t.integer \"scores\", array: true, default: [0]
    t.text \"notes\", array: true
    t.string \"labels\", array: true, default: [\"a\", \"b,c\"]
    t.integer \"mixed\", array: true, default: [\"x\"]
    t.json \"meta\"
    t.index [\"tag_list\"], name: \"index_widgets_on_tag_list\", using: :gin
  end
end
";

const WIDGET: &str = "class Widget < ApplicationRecord
  def self.tagged_by_unnest(tag)
    where(\"id IN (SELECT w.id FROM widgets w, unnest(w.tag_list) AS t(tag) WHERE t.tag = ?)\", tag)
  end

  def self.retag(tags)
    update_all(tag_list: tags, scores: [7, 8])
  end

  def self.remeta(meta)
    update_all(meta: meta)
  end

  def self.bulk_insert(rows)
    insert_all(rows)
  end

  def self.bulk_upsert(rows)
    upsert_all(rows)
  end
end
";

const POSTGRESQL_DATABASE_YML: &str = "default: &default\n  adapter: postgresql\n  encoding: unicode\n\ndevelopment:\n  <<: *default\n  database: widgets_development\n\ntest:\n  <<: *default\n  database: widgets_test\n";

const SQLITE_DATABASE_YML: &str = "test:\n  adapter: sqlite3\n  database: \":memory:\"\n";

fn widget_app(database_yml: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/widget.rb", WIDGET)
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write("db/schema.rb", SCHEMA)
        .write("config/database.yml", database_yml)
}

fn read(emitted: &std::path::Path, path: &str) -> String {
    std::fs::read_to_string(emitted.join(path)).unwrap_or_else(|error| panic!("{path}: {error}"))
}

/// config/schema.rb holds the DDL as Ruby string literals.
fn ddl(emitted: &std::path::Path) -> String {
    read(emitted, "config/schema.rb").replace("\\\"", "\"")
}

/// An array literal default is kept as JSON when each element is of the
/// column's kind: Rails gives a new record `[0]`, not nil. One of
/// another kind is dropped, as a default ingest cannot read.
#[test]
fn ingest_keeps_array_literal_defaults() {
    let tree: HashMap<PathBuf, Vec<u8>> = [
        ("db/schema.rb", SCHEMA),
        ("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n"),
        ("app/models/widget.rb", "class Widget < ApplicationRecord\nend\n"),
        ("config/routes.rb", "Rails.application.routes.draw do\nend\n"),
    ]
    .into_iter()
    .map(|(p, c)| (PathBuf::from(p), c.as_bytes().to_vec()))
    .collect();
    let app = ingest_app_from_tree(tree).expect("ingest");
    let table = app.schema.tables.values().find(|t| t.name.as_str() == "widgets").expect("widgets");
    let default = |name: &str| table.columns.iter().find(|c| c.name.as_str() == name).expect(name).default.clone();
    assert_eq!(default("tag_list").as_deref(), Some("[]"));
    assert_eq!(default("scores").as_deref(), Some("[0]"));
    assert_eq!(default("notes"), None);
    assert_eq!(default("labels").as_deref(), Some("[\"a\",\"b,c\"]"));
    assert_eq!(default("mixed"), None);
}

/// A new record reads each default as the Array it names, on SQLite too.
#[test]
fn a_new_record_reads_array_literal_defaults_on_sqlite() {
    let run = widget_app(SQLITE_DATABASE_YML).run_ruby(
        "w = Widget.new\np [w.tag_list, w.scores, w.notes, w.labels]\nw.save!\nf = Widget.find(w.id)\np [f.tag_list, f.scores, f.notes, f.labels]\n",
    );
    run.assert_passes();
    assert_eq!(run.stdout, "[[], [0], nil, [\"a\", \"b,c\"]]\n[[], [0], nil, [\"a\", \"b,c\"]]\n");
}

#[test]
fn postgresql_ddl_renders_real_arrays() {
    let (emitted, errors) = widget_app(POSTGRESQL_DATABASE_YML).emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let ddl = ddl(&emitted);
    for line in [
        "\"tag_list\" text[] NOT NULL DEFAULT '{}',",
        "\"scores\" integer[] DEFAULT '{0}',",
        "\"notes\" text[],",
        "\"labels\" character varying[] DEFAULT '{\"a\",\"b,c\"}',",
        "\"mixed\" integer[],",
        "ON \"widgets\" USING \"gin\" (\"tag_list\")",
    ] {
        assert!(ddl.contains(line), "{line}\n{ddl}");
    }
}

/// The control: SQLite stores the JSON text, as before.
#[test]
fn sqlite_ddl_keeps_json_text() {
    let (emitted, errors) = widget_app(SQLITE_DATABASE_YML).emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let ddl = ddl(&emitted);
    for line in [
        "tag_list TEXT NOT NULL DEFAULT '[]',",
        "scores TEXT DEFAULT '[0]',",
        "notes TEXT,",
        "labels TEXT DEFAULT '[\"a\",\"b,c\"]',",
    ] {
        assert!(ddl.contains(line), "{line}\n{ddl}");
    }
}

/// The PostgreSQL tree ships the array-literal codec as
/// runtime/array_column.rb; the SQLite tree keeps the JSON one. Neither
/// ships array_column_pg.rb under its own name.
#[test]
fn the_postgresql_tree_ships_the_array_literal_codec() {
    let (emitted, errors) = widget_app(POSTGRESQL_DATABASE_YML).emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let codec = read(&emitted, "runtime/array_column.rb");
    assert!(codec.contains("PostgreSQL's array input format") && !codec.contains("require \"json\""), "{codec}");
    assert!(!emitted.join("runtime/array_column_pg.rb").exists());

    let (emitted, errors) = widget_app(SQLITE_DATABASE_YML).emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    assert!(read(&emitted, "runtime/array_column.rb").contains("JSON.parse"));
    assert!(!emitted.join("runtime/array_column_pg.rb").exists());
}

/// The codec against the server: each value dumped, cast to `text[]` /
/// `integer[]`, read back as the server prints it and loaded, gives the
/// value again. The quoting and escaping cases are the two layers' (the
/// array literal's `\` and `"`, then the SQL string's `'`).
const CODEC_CASES: &str = r##"
require "pg"
require ARGV[1]
conn = PG.connect(ARGV[0])
quote = ->(s) { s.nil? ? "NULL" : "'" + s.gsub("'", "''") + "'" }
text = ->(sql) { conn.exec(sql).getvalue(0, 0) }
[[], ["a"], ["a,b"], ["say \"hi\""], ["back\\slash"], ["it's"], [""], ["NULL"], [" pad "], ["{x}"], ["é"],
 ["a", "b", "c"], ["\\\"", "'\"", ",}{"]].each do |value|
  out = text.("SELECT #{quote.(ArrayColumn.dump(value))}::text[]")
  back = ArrayColumn.load_strings(out)
  raise "#{value.inspect}: #{out} loaded #{back.inspect}" unless back == value
end
[[1, 2], [0], [-3], []].each do |value|
  out = text.("SELECT #{quote.(ArrayColumn.dump(value))}::integer[]")
  back = ArrayColumn.load_ints(out)
  raise "#{value.inspect}: #{out} loaded #{back.inspect}" unless back == value && back.all? { |e| e.is_a?(Integer) }
end
# The server's own output, not one this codec wrote.
back = ArrayColumn.load_strings(text.("SELECT ARRAY['a,b', 'x\"y', 'NULL', '']::text[]"))
raise "server literal: #{back.inspect}" unless back == ["a,b", "x\"y", "NULL", ""]
# nil is NULL; [] is the empty array. A NULL element reads as the
# SQLite lane's JSON null does: "" or 0.
raise "dump nil" unless ArrayColumn.dump(nil).nil? && ArrayColumn.load_strings(nil).nil?
raise "dump []" unless ArrayColumn.dump([]) == "{}" && ArrayColumn.load_strings("{}") == []
back = ArrayColumn.load_strings(text.("SELECT ARRAY['a', NULL]::text[]"))
raise "null element: #{back.inspect}" unless back == ["a", ""]
raise "null int element" unless ArrayColumn.load_ints(text.("SELECT ARRAY[1, NULL]::integer[]")) == [1, 0]
begin
  ArrayColumn.load_strings(text.("SELECT ARRAY[ARRAY['a'], ARRAY['b']]::text[]"))
  raise "a two-dimensional array loaded"
rescue ArgumentError
end
begin
  ArrayColumn.load_strings(text.("SELECT '[0:0]={a}'::text[]"))
  raise "an array with bounds loaded"
rescue ArgumentError
end
print "codec ok"
"##;

#[test]
fn the_codec_round_trips_through_postgresql() {
    let Some(db) = pg_test_url::pg_test_db("postgresql_array_codec") else { return };
    let codec = concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/spinel/array_column_pg.rb");
    let output = emit_and_run::ruby().args(["-e", CODEC_CASES, &db.url, codec]).output().expect("ruby with the pg gem");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "codec ok");
}

/// The app on PostgreSQL: boot creates the table and its GIN index,
/// the defaults and every write go through the model, nil and [] stay
/// distinct, and the column answers `unnest`, `@>` and `pluck`.
const APP_RUN: &str = r##"
w = Widget.new(name: "one")
raise "new #{[w.tag_list, w.scores, w.notes].inspect}" unless w.tag_list == [] && w.scores == [0] && w.notes.nil?
w.save!
f = Widget.find(w.id)
raise "reload #{[f.tag_list, f.scores, f.notes].inspect}" unless f.tag_list == [] && f.scores == [0] && f.notes.nil?

f.tag_list = ["a,b", "q\"t", "it's", "back\\slash"]
f.scores = [3, -1]
f.save!
g = Widget.find(w.id)
raise "write #{g.tag_list.inspect} #{g.scores.inspect}" unless g.tag_list == ["a,b", "q\"t", "it's", "back\\slash"] && g.scores == [3, -1]
g.tag_list << "c"
g.save!
raise "append #{Widget.find(w.id).tag_list.inspect}" unless Widget.find(w.id).tag_list == ["a,b", "q\"t", "it's", "back\\slash", "c"]

g = Widget.find(w.id)
g.notes = []
g.save!
raise "empty #{Widget.find(w.id).notes.inspect}" unless Widget.find(w.id).notes == []
g.notes = nil
g.save!
raise "nil #{Widget.find(w.id).notes.inspect}" unless Widget.find(w.id).notes.nil?

Widget.create!(name: "two", tag_list: ["x"])
tags = Widget.tagged_by_unnest("c")
raise "unnest #{tags.pluck(:name).inspect}" unless tags.pluck(:name) == ["one"]
raise "contains" unless Widget.where("tag_list @> ARRAY[?]::text[]", "a,b").count == 1
raise "contains x" unless Widget.where("tag_list @> ARRAY[?]::text[]", "x").pluck(:name) == ["two"]
plucked = Widget.order(:id).pluck(:tag_list)
raise "pluck #{plucked.inspect}" unless plucked == [["a,b", "q\"t", "it's", "back\\slash", "c"], ["x"]]
raise "pluck ints" unless Widget.order(:id).pluck(:scores) == [[3, -1], [0]]
print "array columns"
"##;

#[test]
fn array_columns_round_trip_on_postgresql() {
    let Some(db) = pg_test_url::pg_test_db("postgresql_array_columns") else { return };
    let on_server = |sql: String| {
        let output = std::process::Command::new("ruby")
            .args(["-rpg", "-e", "PG.connect(ARGV[0]).exec(ARGV[1])", &db.url, &sql])
            .output()
            .expect("ruby with the pg gem");
        assert!(output.status.success(), "{sql}: {}", String::from_utf8_lossy(&output.stderr));
    };
    on_server(format!("CREATE SCHEMA {}", db.schema));
    let options = format!("-c search_path={}", db.schema);
    let run = widget_app(POSTGRESQL_DATABASE_YML)
        .run_ruby_env(APP_RUN, &[("DATABASE_URL", db.url.as_str()), ("PGOPTIONS", options.as_str())]);
    on_server(format!("DROP SCHEMA {} CASCADE", db.schema));
    run.assert_passes();
    assert_eq!(run.stdout, "array columns");
}

const FIXTURES: &str = "one:\n  name: one\n  tag_list: [\"a\", \"b,c\", \"it's\"]\n  scores: [1, 2]\n\ntwo:\n  name: two\n";

const FIXTURE_TEST: &str = r#"require "test_helper"

class WidgetFixtureTest < ActiveSupport::TestCase
  test "a fixture's YAML sequence loads as the Array" do
    assert_equal ["a", "b,c", "it's"], widgets(:one).tag_list
    assert_equal [1, 2], widgets(:one).scores
    assert_nil widgets(:one).notes
    assert_equal [], widgets(:two).tag_list
    assert_equal [0], widgets(:two).scores
  end
end
"#;

fn fixture_app(database_yml: &str) -> emit_and_run::Overlay {
    widget_app(database_yml)
        .write("test/test_helper.rb", "ENV[\"RAILS_ENV\"] ||= \"test\"\nrequire_relative \"../config/environment\"\nrequire \"rails/test_help\"\n\nmodule ActiveSupport\n  class TestCase\n    fixtures :all\n  end\nend\n")
        .write("test/fixtures/widgets.yml", FIXTURES)
        .write("test/models/widget_fixture_test.rb", FIXTURE_TEST)
}

/// A fixture's YAML sequence for an array column is the Array, written
/// through the column's codec: on SQLite (JSON) and PostgreSQL alike.
#[test]
fn a_fixture_array_loads_on_sqlite() {
    fixture_app(SQLITE_DATABASE_YML).run_test("test/models/widget_fixture_test.rb").assert_passes();
}

#[test]
fn a_fixture_array_loads_on_postgresql() {
    let Some(db) = pg_test_url::pg_test_db("postgresql_array_fixtures") else { return };
    fixture_app(POSTGRESQL_DATABASE_YML)
        .run_test_env("test/models/widget_fixture_test.rb", &[("DATABASE_URL", db.url.as_str())])
        .assert_passes();
}

/// The bulk writes that skip the model (`update_all`, `insert_all`,
/// `upsert_all`) write an Array into a stored array column as the
/// column's codec does: the array literal on PostgreSQL, JSON on SQLite.
const BULK_WRITES: &str = r##"
w = Widget.create!(name: "one")
Widget.retag(["x,y", "q\"t", "it's"])
f = Widget.find(w.id)
raise "update_all #{f.tag_list.inspect} #{f.scores.inspect}" unless f.tag_list == ["x,y", "q\"t", "it's"] && f.scores == [7, 8]
Widget.bulk_insert([{ name: "two", tag_list: ["p", "{q}"], scores: [5] }])
two = Widget.where(name: "two").first
raise "insert_all #{two.tag_list.inspect} #{two.scores.inspect}" unless two.tag_list == ["p", "{q}"] && two.scores == [5]
Widget.bulk_upsert([{ id: w.id, name: "one", tag_list: ["u", ""], scores: [] }])
f = Widget.find(w.id)
raise "upsert_all #{f.tag_list.inspect} #{f.scores.inspect}" unless f.tag_list == ["u", ""] && f.scores == []
print "bulk writes"
"##;

/// The control: an Array into a json column stays JSON.
const JSON_BULK_WRITES: &str = r##"
w = Widget.create!(name: "one")
Widget.remeta(["k", 2])
raise "json update_all #{Widget.find(w.id).meta.inspect}" unless Widget.find(w.id).meta == ["k", 2]
Widget.bulk_insert([{ name: "two", meta: [1, "z"] }])
raise "json insert_all #{Widget.where(name: "two").first.meta.inspect}" unless Widget.where(name: "two").first.meta == [1, "z"]
print "bulk writes"
"##;

fn run_on_postgresql(test: &str, script: &str) {
    let Some(db) = pg_test_url::pg_test_db(test) else { return };
    let on_server = |sql: String| {
        let output = std::process::Command::new("ruby")
            .args(["-rpg", "-e", "PG.connect(ARGV[0]).exec(ARGV[1])", &db.url, &sql])
            .output()
            .expect("ruby with the pg gem");
        assert!(output.status.success(), "{sql}: {}", String::from_utf8_lossy(&output.stderr));
    };
    on_server(format!("CREATE SCHEMA {}", db.schema));
    let options = format!("-c search_path={}", db.schema);
    let run = widget_app(POSTGRESQL_DATABASE_YML)
        .run_ruby_env(script, &[("DATABASE_URL", db.url.as_str()), ("PGOPTIONS", options.as_str())]);
    on_server(format!("DROP SCHEMA {} CASCADE", db.schema));
    run.assert_passes();
    assert_eq!(run.stdout, "bulk writes");
}

#[test]
fn bulk_writes_encode_arrays_on_postgresql() {
    run_on_postgresql("postgresql_array_bulk", BULK_WRITES);
}

#[test]
fn bulk_writes_encode_arrays_on_sqlite() {
    let run = widget_app(SQLITE_DATABASE_YML).run_ruby(BULK_WRITES);
    run.assert_passes();
    assert_eq!(run.stdout, "bulk writes");
}

#[test]
fn bulk_writes_keep_json_columns_json_on_postgresql() {
    run_on_postgresql("postgresql_array_bulk_json", JSON_BULK_WRITES);
}

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

const SCHEMA: &str = "ActiveRecord::Schema[8.1].define(version: 1) do
  create_table \"widgets\", force: :cascade do |t|
    t.string \"name\"
    t.text \"tag_list\", array: true, default: [], null: false
    t.integer \"scores\", array: true, default: [0]
    t.text \"notes\", array: true
    t.string \"labels\", array: true, default: [\"a\", \"b,c\"]
    t.integer \"mixed\", array: true, default: [\"x\"]
    t.index [\"tag_list\"], name: \"index_widgets_on_tag_list\", using: :gin
  end
end
";

const WIDGET: &str = "class Widget < ApplicationRecord\n  def self.tagged_by_unnest(tag)\n    where(\"id IN (SELECT w.id FROM widgets w, unnest(w.tag_list) AS t(tag) WHERE t.tag = ?)\", tag)\n  end\nend\n";

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


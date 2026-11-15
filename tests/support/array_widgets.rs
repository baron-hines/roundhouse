//! The widgets app the PostgreSQL array column tests share: a table
//! with stored array columns of each kind (string, integer, nullable,
//! with literal defaults), a json column as the control, a GIN index,
//! and model methods for `unnest` and the bulk writes. The Ruby target's
//! runs are tests/postgresql_array_columns.rs, the Spinel target's
//! tests/spinel_pg_db.rs.

#![allow(dead_code)]

use crate::emit_and_run;

pub const SCHEMA: &str = "ActiveRecord::Schema[8.1].define(version: 1) do
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

pub const WIDGET: &str = "class Widget < ApplicationRecord
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

pub const POSTGRESQL_DATABASE_YML: &str = "default: &default\n  adapter: postgresql\n  encoding: unicode\n\ndevelopment:\n  <<: *default\n  database: widgets_development\n\ntest:\n  <<: *default\n  database: widgets_test\n";

pub const SQLITE_DATABASE_YML: &str = "test:\n  adapter: sqlite3\n  database: \":memory:\"\n";

pub fn widget_app(database_yml: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/widget.rb", WIDGET)
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write("db/schema.rb", SCHEMA)
        .write("config/database.yml", database_yml)
}

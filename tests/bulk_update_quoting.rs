//! Bulk UPDATE uses a quoted physical table name exactly once, including
//! when the model's `_table_sql` already quotes an SQLite keyword.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n",
        )
        .write(
            "app/models/group.rb",
            "class Group < ApplicationRecord\nend\n",
        )
        .write(
            "app/models/widget.rb",
            "class Widget < ApplicationRecord\nend\n",
        )
        .write(
            "db/schema.rb",
            r#"ActiveRecord::Schema.define do
  create_table "groups" do |t|
    t.string "name"
    t.datetime "updated_at"
    t.datetime "read_at"
  end
  create_table "widgets" do |t|
    t.string "name"
    t.datetime "updated_at"
    t.datetime "read_at"
  end
end
"#,
        )
        .write(
            "config/routes.rb",
            "Rails.application.routes.draw do\nend\n",
        )
}

#[test]
fn update_all_quotes_the_table_once() {
    app()
        .run_ruby(
            r#"
[Group, Widget].each do |model|
  table = '"' + model.table_name + '"'
  Db.exec("INSERT INTO #{table} (name) VALUES ('selected'), ('untouched')")
  relation = ActiveRecord::Relation.new(model).where(name: "selected")
  sql = Db.capture_sql do
    raise "affected count" unless relation.update_all(name: "changed") == 1
  end
  raise sql.inspect unless sql.length == 1 && sql.first.start_with?("UPDATE #{table} SET ")
  sql = Db.capture_sql do
    raise "String affected count" unless ActiveRecord::Relation.new(model).where(name: "changed").update_all("name = 'again'") == 1
  end
  raise sql.inspect unless sql.length == 1 && sql.first.start_with?("UPDATE #{table} SET ")
  rows = SqliteAdapter.select_rows("SELECT name FROM #{table} ORDER BY id")
  raise rows.inspect unless rows.map { |row| row["name"] } == ["again", "untouched"]
end
"#,
        )
        .assert_passes();
}

#[test]
fn touch_all_quotes_the_table_once() {
    app()
        .run_ruby(
            r#"
[Group, Widget].each do |model|
  table = '"' + model.table_name + '"'
  Db.exec("INSERT INTO #{table} (name, updated_at) VALUES ('selected', '2000-01-01 00:00:00'), ('untouched', '2000-01-01 00:00:00')")
  relation = ActiveRecord::Relation.new(model).where(name: "selected")
  sql = Db.capture_sql do
    raise "affected count" unless relation.touch_all(:read_at) == 1
  end
  raise sql.inspect unless sql.length == 1 && sql.first.start_with?("UPDATE #{table} SET ")
  rows = SqliteAdapter.select_rows("SELECT updated_at, read_at FROM #{table} ORDER BY id")
  raise rows.inspect unless rows[0]["updated_at"] && rows[0]["updated_at"] != "2000-01-01 00:00:00"
  raise rows.inspect unless rows[0]["updated_at"] == rows[0]["read_at"]
  raise rows.inspect unless rows[1]["updated_at"] == "2000-01-01 00:00:00" && rows[1]["read_at"].nil?
  sql = Db.capture_sql do
    raise "default touch affected count" unless relation.touch_all == 1
  end
  raise sql.inspect unless sql.length == 1 && sql.first.start_with?("UPDATE #{table} SET ")
end
"#,
        )
        .assert_passes();
}

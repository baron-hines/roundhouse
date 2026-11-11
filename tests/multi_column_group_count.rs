#[path = "support/emit_and_run.rs"]
mod emit_and_run;
#[path = "support/prerequisite.rs"]
mod prerequisite;

use std::process::Command;

const SCHEMA: &str = r#"
ActiveRecord::Schema.define do
  create_table "messages", force: :cascade do |t|
    t.integer "recipient"
    t.string "kind"
    t.boolean "seen", null: false, default: false
  end
end
"#;

const MODEL: &str = r#"
class Message < ApplicationRecord
  def self.one
    group(:recipient).count
  end
  def self.two
    group(:recipient, :kind).count
  end
  def self.three
    group(:recipient, :kind, :seen).count
  end
  def self.popular
    group(:recipient, :kind).having("COUNT(*) > 1").count
  end
  def self.unique_kinds
    where("kind IS NOT NULL").select(:kind).group(:recipient, :kind).distinct.count
  end
  def self.unique_popular
    where("kind IS NOT NULL").select(:kind).group(:recipient, :kind).distinct.having("COUNT(*) > 1").count
  end
  def self.empty_groups
    none.group(:recipient, :kind).count
  end
end
"#;

const PROBE: &str = r#"
[[1, "action", false], [1, "action", false], [1, "fyi", true],
 [2, "action", true], [2, "action", false], [3, "action", false], [nil, nil, false]].each do |recipient, kind, seen|
  Message.create!(recipient: recipient, kind: kind, seen: seen)
end
puts Message.one.map { |k, v| [k, v].inspect }.sort
puts Message.two.map { |k, v| [k, v].inspect }.sort
puts Message.three.map { |k, v| [k, v].inspect }.sort
puts Message.popular.map { |k, v| [k, v].inspect }.sort
puts Message.unique_kinds.map { |k, v| [k, v].inspect }.sort
puts Message.unique_popular.map { |k, v| [k, v].inspect }.sort
puts Message.empty_groups.inspect
"#;

#[test]
fn emitted_multi_column_counts_match_rails() {
    let oracle = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/rails_oracle.rb");
    let control = format!(
        r#"
require {oracle:?}
unless RailsOracle.available?("activerecord", sqlite3: ">= 2.9")
  warn "group count control: #{{RailsOracle.label}} or sqlite3 unavailable"
  exit 77
end
RailsOracle.activate("activerecord", sqlite3: ">= 2.9")
require "active_record"
ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
ActiveRecord::Migration.verbose = false
{SCHEMA}
class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
{MODEL}
{PROBE}
"#
    );
    let native = Command::new("ruby").args(["-e", &control]).output().expect("ruby");
    if native.status.code() == Some(77) {
        prerequisite::missing(String::from_utf8_lossy(&native.stderr).trim());
        return;
    }
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    let run = emit_and_run::empty_app()
        .write("db/schema.rb", SCHEMA)
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/message.rb", MODEL)
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write("config/database.yml", "test:\n  adapter: sqlite3\n  database: ':memory:'\n")
        .run_ruby(PROBE);
    run.assert_passes();
    assert_eq!(run.stdout.trim(), String::from_utf8_lossy(&native.stdout).trim());
}

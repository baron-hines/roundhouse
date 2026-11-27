//! A connection answers `unprepared_statement`, and its `to_sql` of a
//! bare Arel node (a predicate built on the model's table) writes its SQL
//! with its values inline, as Rails' adapter does. Ignored as a fork
//! spec until the compiler does; the expected values are ActiveRecord
//! 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/vote.rb", r##"class Vote < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    Vote.create!(choice: "yes", weight: 5)
    Vote.create!(choice: "no", weight: 9)
    votes = Vote.arel_table
    predicate = votes[:choice].eq("yes").and(votes[:weight].eq(5))
    conn = Vote.connection
    sql = conn.unprepared_statement { conn.to_sql(predicate) }
    # Rails: "votes"."choice" = 'yes' AND "votes"."weight" = 5
    check("the values are inline", sql.include?("'yes'") && sql.include?("5"))
    check("the SQL runs as a condition", Vote.where(Arel.sql(sql)).pluck(:choice) == ["yes"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "votes", force: :cascade do |t|
    t.string "choice"
    t.integer "weight"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: relation queries"]
fn a_connection_writes_an_arel_node_unprepared() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: relation queries"]
fn a_connection_writes_an_arel_node_unprepared_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

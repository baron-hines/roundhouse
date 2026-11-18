//! `select(column: value)` aliases a column only when the value is a
//! String or Symbol name; any other value projects nothing, so the
//! records load whole, ids included, as Rails'
//! `arel_column_aliases_from_hash` reads it. Ignored as a fork spec until
//! the compiler does; the expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/kite.rb", r##"class Kite < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    k = Kite.create!(color: "red", size: 4)
    rows = Kite.select(color: 7).to_a
    # Rails: the whole record, its id included
    check("one record", rows.size == 1)
    check("its id", rows.first.id == k.id)
    check("its other columns", rows.first.size == 4)
    # Rails: "red" read under the alias
    check("a Symbol value aliases", Kite.select(color: :shade).first.shade == "red")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "kites", force: :cascade do |t|
    t.string "color"
    t.integer "size"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: relation queries"]
fn a_select_hash_value_that_names_no_alias_projects_nothing() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: relation queries"]
fn a_select_hash_value_that_names_no_alias_projects_nothing_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

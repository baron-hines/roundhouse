//! `select` of a String naming one of the model's columns is that
//! column, quoted and qualified, so a column named after an SQL keyword
//! selects, as Rails' `arel_column` reads it as `table[field]`. Ignored
//! as a fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/preference.rb", r##"class Preference < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    Preference.create!(group: "audio", value: "on", notes: "long")
    columns = Preference.column_names - ["notes"]
    # Rails: SELECT "preferences"."id", "preferences"."group", "preferences"."value" FROM "preferences"
    check("each column quoted and qualified", Preference.select(*columns).to_sql.include?(%q("preferences"."group")))
    row = Preference.select(*columns).first
    check("a keyword-named column selects", row.group == "audio")
    check("the left-out column", row.has_attribute?(:notes) == false)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "preferences", force: :cascade do |t|
    t.string "group"
    t.string "value"
    t.string "notes"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: relation queries"]
fn select_of_a_column_name_string_is_that_column() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: relation queries"]
fn select_of_a_column_name_string_is_that_column_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

//! `pluck` of a String that names no column of the model is SQL as
//! written, while a String or Symbol naming a column is that column, as
//! Rails' `arel_column` reads them. Ignored as a fork spec until the
//! compiler does; the expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/coin.rb", r##"class Coin < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    Coin.create!(name: "penny")
    # Rails: [1], [1], ["penny"], ["penny"]
    check("an Arel.sql literal", Coin.pluck(Arel.sql("1")) == [1])
    check("a literal String", Coin.pluck("1") == [1])
    check("a column String", Coin.pluck("name") == ["penny"])
    check("a column Symbol", Coin.pluck(:name) == ["penny"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "coins", force: :cascade do |t|
    t.string "name"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: relation queries"]
fn pluck_of_a_string_naming_no_column_is_sql() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: relation queries"]
fn pluck_of_a_string_naming_no_column_is_sql_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

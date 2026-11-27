//! A model that enumerates its columns in select statements, by
//! inheriting `enumerate_columns_in_select_statements` or by ignoring a
//! column, writes each column in its loads where another model writes
//! `table.*`, as Rails' `build_select` does. Ignored as a fork spec until
//! the compiler does; the expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/listed_record.rb", r##"class ListedRecord < ApplicationRecord
  self.abstract_class = true
  self.enumerate_columns_in_select_statements = true
end
"##)
        .write("app/models/recipe.rb", r##"class Recipe < ListedRecord
end
"##)
        .write("app/models/plain.rb", r##"class Plain < ApplicationRecord
end
"##)
        .write("app/models/trimmed.rb", r##"class Trimmed < ApplicationRecord
  self.ignored_columns = ["legacy"]
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    Recipe.create!(name: "soup", minutes: 5)
    Trimmed.create!(name: "t")
    # Rails: SELECT "recipes"."id", "recipes"."name", "recipes"."minutes" FROM "recipes"
    recipes = Recipe.all.to_sql
    check("an enumerating model names its columns", recipes.include?("minutes") && !recipes.include?("*"))
    # Rails: SELECT "plains".* FROM "plains"
    check("a plain model", Plain.all.to_sql.include?("*"))
    # Rails: SELECT "trimmeds"."id", "trimmeds"."name" FROM "trimmeds"
    trimmed = Trimmed.all.to_sql
    check("an ignoring model names its columns", trimmed.include?("name") && !trimmed.include?("*") && !trimmed.include?("legacy"))
    check("the attribute reads", Recipe.first.minutes == 5)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "recipes", force: :cascade do |t|
    t.string "name"
    t.integer "minutes"
  end
  create_table "plains", force: :cascade do |t|
    t.string "name"
  end
  create_table "trimmeds", force: :cascade do |t|
    t.string "name"
    t.string "legacy"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: relation queries"]
fn a_model_that_enumerates_columns_writes_them_in_its_loads() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: relation queries"]
fn a_model_that_enumerates_columns_writes_them_in_its_loads_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

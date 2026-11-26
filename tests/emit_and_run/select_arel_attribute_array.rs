//! `select` of an Array of Arel attributes projects each attribute's
//! column, as Rails' `arel_columns` flat_maps the fields. Ignored as a
//! fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/gem_stone.rb", r##"class GemStone < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    g = GemStone.create!(name: "ruby", carats: 3)
    columns = %w[id name].map { |c| GemStone.arel_table[c] }
    row = GemStone.select(columns).first
    # Rails: the id and name, and no carats
    check("each column projected", row.id == g.id && row.name == "ruby")
    check("only those columns", row.has_attribute?(:carats) == false)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "gem_stones", force: :cascade do |t|
    t.string "name"
    t.integer "carats"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: relation queries"]
fn select_of_arel_attributes_projects_each_column() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: relation queries"]
fn select_of_arel_attributes_projects_each_column_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

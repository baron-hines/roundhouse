//! `sum(:col)` of an integer column is an Integer, on a relation and on
//! an association alike, as Rails' Calculations cast it through the
//! column type. Ignored as a fork spec until the compiler does; the
//! expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/orchard.rb", r##"class Orchard < ApplicationRecord
  has_many :trees
end
"##)
        .write("app/models/tree.rb", r##"class Tree < ApplicationRecord
  belongs_to :orchard
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    o = Orchard.create!(name: "east")
    Tree.create!(orchard: o, kind: "pear", fruit: 30)
    Tree.create!(orchard: o, kind: "apple", fruit: 12)
    Tree.create!(orchard: o, kind: "apple", fruit: 20)
    # Rails: 62, Integer
    total = Tree.sum(:fruit)
    check("relation sum", total == 62 && total.is_a?(Integer))
    # Rails: 32, Integer
    apples = o.trees.where(kind: "apple").sum(:fruit)
    check("association sum", apples == 32 && apples.is_a?(Integer))
    # Rails: "62 fruit"
    check("interpolated", "#{total} fruit" == "62 fruit")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "orchards", force: :cascade do |t|
    t.string "name"
  end
  create_table "trees", force: :cascade do |t|
    t.integer "orchard_id"
    t.string "kind"
    t.integer "fruit"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: record queries"]
fn the_sum_of_an_integer_column_is_an_integer() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: record queries"]
fn the_sum_of_an_integer_column_is_an_integer_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

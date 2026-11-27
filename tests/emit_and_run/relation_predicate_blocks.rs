//! `any?`, `none?`, `one?`, `many?` and `count` given a block, on a
//! has_many reader or any relation, ask the block about each loaded
//! record, as Rails' `Relation` hands a block form to its records. Without
//! the block they still ask the database. The expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/garden.rb", r##"class Garden < ApplicationRecord
  has_many :plants
end
"##)
        .write("app/models/plant.rb", r##"class Plant < ApplicationRecord
  belongs_to :garden
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    g = Garden.create!(name: "walled")
    Plant.create!(name: "fern", height: 40, garden: g)
    Plant.create!(name: "oak", height: 900, garden: g)
    Plant.create!(name: "moss", height: 2, garden: g)
    # Rails: the block decides, over the association's records
    check("any? with a block", g.plants.any? { |p| p.height > 500 } && !g.plants.any? { |p| p.height > 5000 })
    check("none? with a block", g.plants.none? { |p| p.height > 5000 } && !g.plants.none? { |p| p.height > 500 })
    check("one? with a block", g.plants.one? { |p| p.height > 500 } && !g.plants.one? { |p| p.height > 10 })
    check("many? with a block", g.plants.many? { |p| p.height > 10 } && !g.plants.many? { |p| p.height > 500 })
    check("count with a block", g.plants.count { |p| p.height > 10 } == 2)
    tall = Plant.where("height > ?", 30)
    check("relation any? with a block", !tall.any? { |p| p.name == "moss" } && tall.any? { |p| p.name == "oak" })
    check("relation count with a block", tall.count { |p| p.name.length == 4 } == 1)
    check("relation none?, one? and many? with a block", tall.none? { |p| p.height < 10 } && tall.one? { |p| p.height > 500 } && !tall.many? { |p| p.height > 500 })
    # Rails: neither asks the block again once a second record answers
    asked = 0
    check("one? stops at a second match", !g.plants.one? { |p| asked += 1; true } && asked == 2)
    asked = 0
    check("many? stops at a second match", g.plants.many? { |p| asked += 1; true } && asked == 2)
    # Rails: without a block they still ask about the rows
    check("any? without a block", g.plants.any? && !Plant.where(height: 1).any?)
    check("count without a block", g.plants.count == 3 && tall.count == 2)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "gardens", force: :cascade do |t|
    t.string "name"
  end
  create_table "plants", force: :cascade do |t|
    t.string "name"
    t.integer "height"
    t.integer "garden_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn relation_predicates_with_a_block_ask_the_block() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn relation_predicates_with_a_block_ask_the_block_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

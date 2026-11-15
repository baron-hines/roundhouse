//! A has_many :through whose through association carries a scope reads
//! that scope's conditions and orderings into its query, on the through
//! model's table, and leaves the rest of it (a limit) behind, as Rails'
//! `AssociationScope` merges a non-head reflection's scope. The probe's
//! expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/garden.rb", r##"class Garden < ApplicationRecord
  has_many :beds, -> { where(fallow: false).order(:position) }
  has_many :plants, through: :beds
  has_many :ranked_beds, -> { where(fallow: false).order(position: :desc) }, class_name: "Bed"
  has_many :ranked_plants, through: :ranked_beds, source: :plants
  has_many :all_beds, class_name: "Bed"
  has_many :any_plants, through: :all_beds, source: :plants
  has_many :first_beds, -> { order(:position).limit(1) }, class_name: "Bed"
  has_many :first_bed_plants, through: :first_beds, source: :plants
  has_many :tended_beds, -> { tended }, class_name: "Bed"
  has_many :tended_plants, through: :tended_beds, source: :plants
  has_many :named_plants, -> { order(:name) }, through: :beds, source: :plants
  has_many :two_key_beds, -> { where(fallow: false).order(position: :asc, id: :asc) }, class_name: "Bed"
  has_many :two_key_plants, through: :two_key_beds, source: :plants
end
"##)
        .write("app/models/bed.rb", r##"class Bed < ApplicationRecord
  belongs_to :garden
  has_many :plants
  scope :tended, -> { where(fallow: false) }
end
"##)
        .write("app/models/plant.rb", r##"class Plant < ApplicationRecord
  belongs_to :bed
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    g = Garden.create!(name: "walled")
    north = Bed.create!(garden: g, position: 2, fallow: false)
    south = Bed.create!(garden: g, position: 1, fallow: false)
    idle = Bed.create!(garden: g, position: 0, fallow: true)
    Plant.create!(bed: north, name: "kale", position: 0)
    Plant.create!(bed: south, name: "leek", position: 9)
    Plant.create!(bed: idle, name: "weed", position: 3)
    Plant.create!(bed: south, name: "bean", position: 1)
    g = Garden.find(g.id)
    # Rails: the fallow bed's plant left out, rows in the beds' position order
    check("through scope", g.plants.map(&:name)[0, 2].sort == ["bean", "leek"] && g.plants.map(&:name).last == "kale")
    check("through scope count", g.plants.count == 3)
    check("unscoped through", g.any_plants.map(&:name).sort == ["bean", "kale", "leek", "weed"])
    # Rails: the through scope's limit does not reach the target query
    check("limit left behind", g.first_bed_plants.map(&:name).sort == ["bean", "kale", "leek", "weed"])
    check("named scope", g.tended_plants.map(&:name).sort == ["bean", "kale", "leek"])
    # Rails: the hash ordering names the beds table too, so it sorts by the
    # beds' position — the plants' own position would put leek first
    ranked = g.ranked_plants.map(&:name)
    check("hash order on the through table", ranked.first == "kale" && ranked.sort == ["bean", "kale", "leek"])
    # Rails: the reader's own ordering first, the through scope's after it
    check("own order first", g.named_plants.map(&:name) == ["bean", "kale", "leek"])
    check("preloaded", Garden.where(id: g.id).preload(:plants).first.plants.map(&:name).sort == ["bean", "kale", "leek"])
    # Rails: a hash ordering of two through-table columns qualifies each
    # one, so the rows sort by the beds' position and then the beds' id —
    # the plants' own `position` would order them high, dup, low.
    t = Garden.create!(name: "two keys")
    low = Bed.create!(garden: t, position: 1, fallow: false)
    dup = Bed.create!(garden: t, position: 1, fallow: false)
    high = Bed.create!(garden: t, position: 2, fallow: false)
    Plant.create!(bed: low, name: "low", position: 9)
    Plant.create!(bed: dup, name: "dup", position: 5)
    Plant.create!(bed: high, name: "high", position: 0)
    t = Garden.find(t.id)
    check("two-key through order", t.two_key_plants.map(&:name) == ["low", "dup", "high"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "gardens", force: :cascade do |t|
    t.string "name"
  end
  create_table "beds", force: :cascade do |t|
    t.integer "garden_id"
    t.integer "position"
    t.boolean "fallow", default: false
  end
  create_table "plants", force: :cascade do |t|
    t.integer "bed_id"
    t.string "name"
    t.integer "position"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_through_reader_merges_its_through_scope_as_rails_does() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_through_reader_merges_its_through_scope_as_rails_does_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

//! Loading a relation built off a has_many (`owner.items.where(...)`)
//! seats the owner on each row it loads, and leaves the owner's own
//! loaded target as it was, as Rails' `AssociationRelation` sets the
//! inverse from queries. The expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/fleet.rb", r##"class Fleet < ApplicationRecord
  has_many :ships
end
"##)
        .write("app/models/flotilla.rb", r##"class Flotilla < Fleet
  has_many :buoys
end
"##)
        .write("app/models/buoy.rb", r##"class Buoy < ApplicationRecord
  belongs_to :flotilla
end
"##)
        .write("app/models/ship.rb", r##"class Ship < ApplicationRecord
  belongs_to :fleet

  def self.heavy
    where("tons > ?", 10)
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    f = Fleet.create!(name: "blue")
    Ship.create!(name: "ketch", fleet: f, tons: 5)
    Ship.create!(name: "brig", fleet: f, tons: 40)
    f = Fleet.find(f.id)
    held = f.ships.to_a
    big = f.ships.where("tons > ?", 10).to_a
    # Rails: the owner object on the queried row, the target untouched
    check("one row", big.length == 1 && big.first.name == "brig")
    check("owner seated", big.first.fleet.equal?(f))
    check("target untouched", f.ships.length == 2 && f.ships.map(&:name).sort == ["brig", "ketch"])
    check("queried row is its own object", !held.any? { |s| s.equal?(big.first) })
    # Rails: every load off the association's relation seats the owner
    check("ordered first", f.ships.order(:tons).first.fleet.equal?(f))
    check("find_by", f.ships.find_by(name: "ketch").fleet.equal?(f))
    # Rails: a row whose select left the key out is not seated, and loads
    sel = f.ships.where(tons: 40).select(:name).to_a
    check("partial select", sel.map(&:name) == ["brig"] && !sel.first.association(:fleet).loaded?)
    # Rails: a class method reached through the association runs on its relation
    check("class method", f.ships.heavy.to_a.map(&:name) == ["brig"] && f.ships.heavy.first.fleet.equal?(f))
    seen = []
    f.ships.where("tons < ?", 100).each { |s| seen << s.fleet }
    check("each", seen.length == 2 && seen.all? { |o| o.equal?(f) })
    # Rails: a row whose key names another owner is not seated
    other = Fleet.create!(name: "red")
    Ship.create!(name: "yawl", fleet: other, tons: 3)
    all = f.ships.unscope(where: :fleet_id).order(:id).to_a
    check("another owner's row", all.length == 3 && all.last.fleet.name == "red" && !all.last.fleet.equal?(f))
    # Rails: a subclass's relation off an inherited has_many seats it too,
    # beside the subclass's own has_many
    fl = Flotilla.create!(name: "grey")
    Ship.create!(name: "skiff", fleet: fl, tons: 1)
    fl = Flotilla.find(fl.id)
    check("inherited has_many", fl.ships.where(name: "skiff").first.fleet.equal?(fl))
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "fleets", force: :cascade do |t|
    t.string "name"
    t.string "type"
  end
  create_table "buoys", force: :cascade do |t|
    t.string "color"
    t.integer "flotilla_id"
  end
  create_table "ships", force: :cascade do |t|
    t.string "name"
    t.integer "tons"
    t.integer "fleet_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn an_association_relation_load_seats_the_owner_on_its_rows() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_association_relation_load_seats_the_owner_on_its_rows_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

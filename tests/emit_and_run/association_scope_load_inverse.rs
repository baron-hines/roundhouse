//! Loading a relation built off a has_many (`owner.items.where(...)`)
//! seats the owner on each row it loads, and leaves the owner's own
//! loaded target as it was, as Rails' `AssociationRelation` sets the
//! inverse from queries. Ignored as a fork spec until the compiler does;
//! the expected values are ActiveRecord 8.1.4's answers.

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
        .write("app/models/ship.rb", r##"class Ship < ApplicationRecord
  belongs_to :fleet
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
    check("target untouched", f.ships.target.length == 2 && f.ships.target.map(&:name).sort == ["brig", "ketch"])
    check("queried row is its own object", !held.any? { |s| s.equal?(big.first) })
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "fleets", force: :cascade do |t|
    t.string "name"
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
#[ignore = "fork spec: association targets"]
fn an_association_relation_load_seats_the_owner_on_its_rows() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: association targets"]
fn an_association_relation_load_seats_the_owner_on_its_rows_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

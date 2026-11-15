//! `includes` of a has_one or a belongs_to whose table a where hash names
//! is an eager load: one query joins the association, filters on it, and
//! hands each owner its record, as Rails' `references_eager_loaded_tables?`
//! decides. Ignored as a fork spec until the compiler does; the expected
//! values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/farm.rb", r##"class Farm < ApplicationRecord
  has_one :barn
end
"##)
        .write("app/models/barn.rb", r##"class Barn < ApplicationRecord
  belongs_to :farm
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    a = Farm.create!(name: "upper")
    b = Farm.create!(name: "lower")
    Barn.create!(color: "red", farm: a)
    Barn.create!(color: "grey", farm: b)
    farms = Farm.includes(:barn).where(barns: { color: "red" }).to_a
    # Rails: only the farm with the red barn, its barn in hand
    check("has_one filtered", farms.map(&:name) == ["upper"])
    check("has_one loaded", farms.first.association(:barn).loaded?)
    check("has_one record", farms.first.barn.color == "red")
    barns = Barn.includes(:farm).where(farms: { name: "lower" }).to_a
    check("belongs_to filtered", barns.map(&:color) == ["grey"])
    check("belongs_to loaded", barns.first.association(:farm).loaded?)
    check("belongs_to record", barns.first.farm.name == "lower")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "farms", force: :cascade do |t|
    t.string "name"
  end
  create_table "barns", force: :cascade do |t|
    t.string "color"
    t.integer "farm_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: preload"]
fn includes_of_a_singular_association_a_where_names_eager_loads() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: preload"]
fn includes_of_a_singular_association_a_where_names_eager_loads_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

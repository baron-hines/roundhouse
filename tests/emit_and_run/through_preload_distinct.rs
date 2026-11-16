//! A has_many :through whose scope is `distinct` preloads its hop on the
//! owners, then its source records in one query of their own, as Rails'
//! through preloader does: each owner holds its records once, and the
//! hop records do not hold their source, which the non-empty scope keeps
//! from being seated there. Such a through used to have no batch at all,
//! so every owner's read asked again. The probe's expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/crew.rb", r##"class Crew < ApplicationRecord
  has_many :shifts
  has_many :workers, -> { distinct }, through: :shifts
  belongs_to :depot, optional: true
  has_many :kit, -> { distinct }, through: :depot, source: :tools
end
"##)
        .write("app/models/depot.rb", r##"class Depot < ApplicationRecord
  has_many :tools
end
"##)
        .write("app/models/tool.rb", r##"class Tool < ApplicationRecord
  belongs_to :depot
end
"##)
        .write("app/models/shift.rb", r##"class Shift < ApplicationRecord
  belongs_to :crew
  belongs_to :worker
end
"##)
        .write("app/models/worker.rb", r##"class Worker < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.queries
    count = 0
    counter = ->(event) { count += 1 unless event.payload[:name] == "SCHEMA" }
    ActiveSupport::Notifications.subscribed(counter, "sql.active_record") do
      yield
    end
    count
  end

  def self.run
    day = Crew.create!(name: "day")
    night = Crew.create!(name: "night")
    Crew.create!(name: "idle")
    kim = Worker.create!(name: "kim")
    lee = Worker.create!(name: "lee")
    Shift.create!(crew: day, worker: lee)
    Shift.create!(crew: day, worker: kim)
    Shift.create!(crew: day, worker: lee)
    Shift.create!(crew: night, worker: kim)
    crews = nil
    # Rails: the crews, their shifts, the workers
    check("one query each", queries { crews = Crew.order(:id).preload(:workers).to_a } == 3)
    check("hop held", crews.all? { |c| c.association(:shifts).loaded? && c.association(:workers).loaded? })
    names = nil
    check("read without a query", queries { names = crews.map { |c| c.workers.map(&:name) } } == 0)
    # Rails: each worker once, in shift order
    check("distinct", names == [["lee", "kim"], ["kim"], []])
    check("hop records without the source", crews.all? { |c| c.shifts.none? { |s| s.association(:worker).loaded? } })
    check("hop source read", queries { crews.first.shifts.first.worker.name } == 1)
    yard = Depot.create!(name: "yard")
    Tool.create!(depot: yard, name: "saw")
    Tool.create!(depot: yard, name: "drill")
    day.update!(depot: yard)
    night.update!(depot: yard)
    # Rails: the crews, their depot, the tools, over a belongs_to hop
    check("has_many source", queries { crews = Crew.order(:id).preload(:kit).to_a } == 3)
    check("has_many source read", queries { names = crews.map { |c| c.kit.map(&:name) } } == 0)
    check("has_many source rows", names == [["saw", "drill"], ["saw", "drill"], []])
    check("hop record without its rows", crews.none? { |c| c.depot && c.depot.association(:tools).loaded? })
    crews = Crew.order(:id).preload(shifts: :worker).to_a
    held = crews.first.shifts.first.worker
    # Rails: shifts already holding their workers answer them, with no query
    check("held sources", queries { ActiveRecord::Associations::Preloader.new(records: crews, associations: :workers).call } == 0)
    check("held source answers", crews.first.workers.to_a.first.equal?(held) && crews.first.workers.map(&:name) == ["lee", "kim"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "crews", force: :cascade do |t|
    t.string "name"
    t.integer "depot_id"
  end
  create_table "depots", force: :cascade do |t|
    t.string "name"
  end
  create_table "tools", force: :cascade do |t|
    t.integer "depot_id"
    t.string "name"
  end
  create_table "shifts", force: :cascade do |t|
    t.integer "crew_id"
    t.integer "worker_id"
  end
  create_table "workers", force: :cascade do |t|
    t.string "name"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_distinct_through_preloads_its_hop_then_its_records_once() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_distinct_through_preloads_its_hop_then_its_records_once_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

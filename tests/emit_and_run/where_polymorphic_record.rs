//! `where(<polymorphic belongs_to>: record)` compares the type column with
//! the record's `polymorphic_name` (its STI base class's name) and the key
//! column with its id, also for a class that declares no `as:`
//! association back, as Rails' `PolymorphicArrayValue` writes it; the
//! polymorphic writer stores that name too. Ignored as a fork spec until
//! the compiler does both; the expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/vehicle.rb", r##"class Vehicle < ApplicationRecord
end
"##)
        .write("app/models/truck.rb", r##"class Truck < Vehicle
end
"##)
        .write("app/models/depot.rb", r##"class Depot < ApplicationRecord
end
"##)
        .write("app/models/inspection.rb", r##"class Inspection < ApplicationRecord
  belongs_to :subject, polymorphic: true, optional: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    t = Truck.create!(plate: "T1")
    d = Depot.create!(name: "D1")
    Inspection.create!(result: "pass", subject_type: "Vehicle", subject_id: t.id)
    Inspection.create!(result: "fail", subject_type: "Depot", subject_id: d.id)
    Inspection.create!(result: "odd", subject_type: "Truck", subject_id: t.id)
    # Rails: the type compared is the base class's name
    check("subclass record", Inspection.where(subject: t).pluck(:result) == ["pass"])
    check("plain record", Inspection.where(subject: d).pluck(:result) == ["fail"])
    check("list", Inspection.where(subject: [t, d]).order(:result).pluck(:result) == ["fail", "pass"])
    # Rails: and the writer stores it
    check("written as the base class", Inspection.create!(result: "new", subject: t).subject_type == "Vehicle")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "vehicles", force: :cascade do |t|
    t.string "type"
    t.string "plate"
  end
  create_table "depots", force: :cascade do |t|
    t.string "name"
  end
  create_table "inspections", force: :cascade do |t|
    t.string "result"
    t.string "subject_type"
    t.integer "subject_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: association targets"]
fn where_polymorphic_record_compares_its_polymorphic_name() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: association targets"]
fn where_polymorphic_record_compares_its_polymorphic_name_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

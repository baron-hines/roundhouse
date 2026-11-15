//! A belongs_to's reader that loads its target seats the record on the
//! target's inverse: a has_one inverse answers the record itself without
//! a query, and a has_many inverse it names holds it (with
//! `has_many_inversing`, as Rails 8.1 defaults it), as Rails'
//! `find_target` sets the inverse instance. Ignored as a fork spec until
//! the compiler does; the expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/school.rb", r##"class School < ApplicationRecord
  has_one :principal
  has_many :classrooms
end
"##)
        .write("app/models/principal.rb", r##"class Principal < ApplicationRecord
  belongs_to :school
end
"##)
        .write("app/models/classroom.rb", r##"class Classroom < ApplicationRecord
  belongs_to :school, inverse_of: :classrooms
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    s = School.create!(name: "elm")
    Principal.create!(name: "ito", school: s)
    Classroom.create!(room: "1a", school: s)
    Classroom.create!(room: "2b", school: s)

    p = Principal.first
    # Rails: the has_one inverse is the very record that loaded the school
    check("has_one inverse seated", p.school.association(:principal).loaded? && p.school.principal.equal?(p))

    c = Classroom.find_by(room: "2b")
    c.room = "2c"
    # Rails: the has_many inverse holds the record and keeps it when it loads
    rooms = c.school.classrooms.to_a
    check("has_many inverse holds it", rooms.any? { |r| r.equal?(c) })
    check("unsaved change kept", rooms.map(&:room).sort == ["1a", "2c"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "schools", force: :cascade do |t|
    t.string "name"
  end
  create_table "principals", force: :cascade do |t|
    t.string "name"
    t.integer "school_id"
  end
  create_table "classrooms", force: :cascade do |t|
    t.string "room"
    t.integer "school_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: association targets"]
fn a_belongs_to_load_seats_the_record_on_its_inverse() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: association targets"]
fn a_belongs_to_load_seats_the_record_on_its_inverse_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

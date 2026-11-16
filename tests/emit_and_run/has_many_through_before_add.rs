//! A has_many :through writer (`<<`, and `=` on a new owner) builds each
//! join record through the through association, so that association's
//! `before_add` callback runs once per join record, as Rails'
//! `build_through_record` does. Ignored as a fork spec until the compiler
//! does; the expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/choir.rb", r##"class Choir < ApplicationRecord
  has_many :seats, before_add: :count_seat
  has_many :singers, through: :seats

  def count_seat(seat)
    @seats_built = seats_built + 1
  end

  def seats_built
    @seats_built || 0
  end
end
"##)
        .write("app/models/seat.rb", r##"class Seat < ApplicationRecord
  belongs_to :choir
  belongs_to :singer
end
"##)
        .write("app/models/singer.rb", r##"class Singer < ApplicationRecord
  has_many :seats
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    c = Choir.create!(name: "east")
    c.singers << Singer.create!(name: "alto")
    c.singers << Singer.create!(name: "bass")
    # Rails: 2 join records built, 2 callbacks
    check("concat runs before_add per join record", c.seats_built == 2)
    check("join rows saved", Seat.where(choir_id: c.id).count == 2)

    d = Choir.new(name: "west")
    d.singers = [Singer.create!(name: "tenor"), Singer.create!(name: "soprano"), Singer.create!(name: "mezzo")]
    d.save!
    # Rails: 3
    check("writer runs before_add per join record", d.seats_built == 3)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "choirs", force: :cascade do |t|
    t.string "name"
  end
  create_table "singers", force: :cascade do |t|
    t.string "name"
  end
  create_table "seats", force: :cascade do |t|
    t.integer "choir_id"
    t.integer "singer_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: association targets"]
fn a_through_writer_runs_the_through_before_add_per_join_record() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: association targets"]
fn a_through_writer_runs_the_through_before_add_per_join_record_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

//! An `eager_load` of an association a nested `joins` already JOINs
//! reads that JOIN instead of joining the table a second time, as Rails'
//! JoinDependency merges the two trees. Ignored as a fork spec until the
//! compiler does; the expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/venue.rb", r##"class Venue < ApplicationRecord
end
"##)
        .write("app/models/concert.rb", r##"class Concert < ApplicationRecord
  belongs_to :venue, optional: true
end
"##)
        .write("app/models/seat.rb", r##"class Seat < ApplicationRecord
end
"##)
        .write("app/models/ticket.rb", r##"class Ticket < ApplicationRecord
  belongs_to :concert, optional: true
  belongs_to :seat, optional: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    hall = Venue.create!(name: "hall")
    gig = Concert.create!(venue: hall, title: "gig")
    seat = Seat.create!(row: "a")
    Ticket.create!(concert: gig, seat: seat, code: "t1")
    statements = []
    counter = ->(event) { statements << event.payload[:sql] unless event.payload[:name] == "SCHEMA" }
    rows = nil
    ActiveSupport::Notifications.subscribed(counter, "sql.active_record") do
      rows = Ticket.joins(concert: :venue).eager_load(:concert, :seat).where(venues: { name: "hall" }).to_a
    end
    # Rails: one statement, one JOIN of concerts
    check("concerts joined once", statements.size == 1 && statements.first.scan(/JOIN "?concerts"? /).size == 1)
    first = rows.first
    check("loaded by the eager load", first.association(:concert).loaded? && first.association(:seat).loaded?)
    check("the eager reads", rows.map { |t| [t.code, t.concert.title, t.seat.row] } == [["t1", "gig", "a"]])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "venues", force: :cascade do |t|
    t.string "name"
  end
  create_table "concerts", force: :cascade do |t|
    t.integer "venue_id"
    t.string "title"
  end
  create_table "seats", force: :cascade do |t|
    t.string "row"
  end
  create_table "tickets", force: :cascade do |t|
    t.integer "concert_id"
    t.integer "seat_id"
    t.string "code"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: relation queries"]
fn an_eager_load_reads_the_join_a_nested_joins_made() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: relation queries"]
fn an_eager_load_reads_the_join_a_nested_joins_made_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

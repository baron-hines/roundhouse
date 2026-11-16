//! A has_one :through or has_many :through with no scope preloads its hop
//! on the owners first, then its source off the hop records, as Rails'
//! through preloader does: one query for the hop, one for the source, the
//! hop held by each owner and the source by each hop record, and the
//! owner's records in hop order. An owner holding its hop or its target
//! keeps it, and nothing runs when every owner holds its target. A
//! has_one :through preload used to run as a direct has_one, on an owner
//! column the target does not have; a has_many :through one joined the
//! target to the hop and left the hop records unloaded. The probe's
//! expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/team.rb", r##"class Team < ApplicationRecord
  has_many :memberships
  has_many :players, through: :memberships
end
"##)
        .write("app/models/membership.rb", r##"class Membership < ApplicationRecord
  belongs_to :team
  belongs_to :player
end
"##)
        .write("app/models/player.rb", r##"class Player < ApplicationRecord
end
"##)
        .write("app/models/venue.rb", r##"class Venue < ApplicationRecord
  has_many :events
  has_many :tickets, through: :events
end
"##)
        .write("app/models/event.rb", r##"class Event < ApplicationRecord
  belongs_to :venue
  has_many :tickets
end
"##)
        .write("app/models/stub.rb", r##"class Stub < ApplicationRecord
  belongs_to :ticket
  has_many :scans
end
"##)
        .write("app/models/scan.rb", r##"class Scan < ApplicationRecord
  belongs_to :stub
end
"##)
        .write("app/models/ticket.rb", r##"class Ticket < ApplicationRecord
  belongs_to :event
  has_one :venue, through: :event
  has_one :stub
  has_many :scans, through: :stub
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
    red = Team.create!(name: "red")
    blue = Team.create!(name: "blue")
    ann = Player.create!(name: "ann")
    bob = Player.create!(name: "bob")
    Membership.create!(team: red, player: bob)
    Membership.create!(team: red, player: ann)
    Membership.create!(team: red, player: bob)
    Membership.create!(team: blue, player: ann)
    teams = nil
    # Rails: the teams, their memberships, the players
    check("has_many hop, one query each", queries { teams = Team.order(:id).preload(:players).to_a } == 3)
    check("hop held", teams.all? { |t| t.association(:memberships).loaded? && t.association(:players).loaded? })
    names = nil
    check("read without a query", queries { names = teams.map { |t| t.players.map(&:name) } } == 0)
    # Rails: in membership order, a player twice for two memberships
    check("hop order", names == [["bob", "ann", "bob"], ["ann"]])
    check("hop records hold the source", queries { teams.each { |t| t.memberships.each { |m| m.player.name } } } == 0)
    teams = Team.order(:id).preload(:memberships).to_a
    # Rails: the held memberships are not asked again
    check("held hop", queries { ActiveRecord::Associations::Preloader.new(records: teams, associations: :players).call } == 1)
    check("held hop read", teams.map { |t| t.players.map(&:name) } == [["bob", "ann", "bob"], ["ann"]])

    hall = Venue.create!(name: "hall")
    barn = Venue.create!(name: "barn")
    gig = Event.create!(name: "gig", venue: hall)
    fair = Event.create!(name: "fair", venue: barn)
    a1 = Ticket.create!(seat: "a1", event: gig)
    Ticket.create!(seat: "b2", event: fair)
    Ticket.create!(seat: "c3", event: gig)
    stub = Stub.create!(ticket: a1)
    Scan.create!(stub: stub, at: "door")
    Scan.create!(stub: stub, at: "bar")
    tickets = nil
    # Rails: the tickets, their events, the venues
    check("belongs_to hop, one query each", queries { tickets = Ticket.order(:id).preload(:venue).to_a } == 3)
    check("belongs_to hop held", tickets.all? { |t| t.association(:event).loaded? && t.association(:venue).loaded? })
    check("has_one read without a query", queries { names = tickets.map { |t| t.venue.name } } == 0)
    check("has_one targets", names == ["hall", "barn", "hall"])
    check("hop holds the has_one source", queries { tickets.each { |t| t.event.venue.name } } == 0)
    # Rails: every owner holds its target, so nothing runs
    check("all held", queries { ActiveRecord::Associations::Preloader.new(records: tickets, associations: :venue).call } == 0)
    # Rails: the tickets, their stubs, the scans
    check("has_one hop", queries { tickets = Ticket.order(:id).preload(:scans).to_a } == 3)
    check("has_one hop read", queries { names = tickets.map { |t| t.scans.map(&:at) } } == 0)
    check("has_one hop rows", names == [["door", "bar"], [], []])
    venues = nil
    # Rails: the venues, their events, the tickets
    check("has_many source", queries { venues = Venue.order(:id).preload(:tickets).to_a } == 3)
    check("has_many source read", queries { names = venues.map { |v| v.tickets.map(&:seat) } } == 0)
    check("has_many source rows", names == [["a1", "c3"], ["b2"]])
    check("hop records hold the rows", venues.all? { |v| v.events.all? { |e| e.association(:tickets).loaded? } })
    venues = Venue.order(:id).preload(events: :tickets).to_a
    held = venues.first.events.first.tickets.first
    held.seat = "edited"
    # Rails: the held events and their held tickets answer, with no query
    check("held hop and source", queries { ActiveRecord::Associations::Preloader.new(records: venues, associations: :tickets).call } == 0)
    check("held source kept", venues.first.tickets.to_a.first.equal?(held) && venues.first.tickets.to_a.first.seat == "edited")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "teams", force: :cascade do |t|
    t.string "name"
  end
  create_table "memberships", force: :cascade do |t|
    t.integer "team_id"
    t.integer "player_id"
  end
  create_table "players", force: :cascade do |t|
    t.string "name"
  end
  create_table "venues", force: :cascade do |t|
    t.string "name"
  end
  create_table "events", force: :cascade do |t|
    t.string "name"
    t.integer "venue_id"
  end
  create_table "tickets", force: :cascade do |t|
    t.string "seat"
    t.integer "event_id"
  end
  create_table "stubs", force: :cascade do |t|
    t.integer "ticket_id"
  end
  create_table "scans", force: :cascade do |t|
    t.integer "stub_id"
    t.string "at"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_through_preloads_its_hop_then_its_source() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_through_preloads_its_hop_then_its_source_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

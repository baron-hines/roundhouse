//! A has_one :through reads its record through the hop, and a has_many
//! :through over a belongs_to or has_one hop reads its rows through it,
//! as Rails' through association scope joins them. Both used to keep the
//! direct reader, which asked the target for an owner column it does not
//! have. A hop no join can follow (a polymorphic belongs_to) is an
//! `unsupported` error instead. The probe's expected values are
//! ActiveRecord 8.1.4's answers.

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
        .write("app/models/event.rb", r##"class Event < ApplicationRecord
  belongs_to :venue
  has_one :poster
  has_many :tickets
end
"##)
        .write("app/models/poster.rb", r##"class Poster < ApplicationRecord
  belongs_to :event
end
"##)
        .write("app/models/gate.rb", r##"class Gate < ApplicationRecord
end
"##)
        .write("app/models/stub.rb", r##"class Stub < ApplicationRecord
  belongs_to :ticket
  belongs_to :gate
  has_many :scans
end
"##)
        .write("app/models/scan.rb", r##"class Scan < ApplicationRecord
  belongs_to :stub
end
"##)
        .write("app/models/ticket.rb", r##"class Ticket < ApplicationRecord
  belongs_to :event, optional: true
  has_one :venue, through: :event
  has_one :open_venue, -> { where(open: true) }, through: :event, source: :venue
  has_one :poster, through: :event
  has_many :companions, through: :event, source: :tickets
  has_one :stub
  has_one :gate, through: :stub
  has_many :scans, through: :stub
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    hall = Venue.create!(name: "hall", open: true)
    barn = Venue.create!(name: "barn", open: false)
    gig = Event.create!(name: "gig", venue: hall)
    fair = Event.create!(name: "fair", venue: barn)
    Poster.create!(event: gig, title: "tonight")
    a1 = Ticket.create!(seat: "a1", event: gig)
    b2 = Ticket.create!(seat: "b2", event: fair)
    c3 = Ticket.create!(seat: "c3", event: gig)
    loose = Ticket.create!(seat: "d4")
    north = Gate.create!(name: "north")
    stub = Stub.create!(ticket: a1, gate: north)
    Scan.create!(stub: stub, at: "door")
    Scan.create!(stub: stub, at: "bar")
    a1 = Ticket.find(a1.id)
    b2 = Ticket.find(b2.id)
    # Rails: the record the belongs_to hop names, and its own scope
    check("through a belongs_to hop", a1.venue.name == "hall" && b2.venue.name == "barn")
    check("own scope", a1.open_venue.name == "hall" && b2.open_venue.nil?)
    check("has_one source", a1.poster.title == "tonight" && b2.poster.nil?)
    check("has_many source", a1.companions.map(&:seat).sort == ["a1", "c3"] && b2.companions.map(&:seat) == ["b2"])
    check("no hop", Ticket.find(loose.id).venue.nil?)
    # Rails: an unsaved owner whose belongs_to key is set reads through it
    check("unsaved owner", Ticket.new(event: fair).venue.name == "barn" && Ticket.new.venue.nil?)
    # Rails: the row the has_one hop holds, and the rows under it
    check("through a has_one hop", a1.gate.name == "north" && b2.gate.nil?)
    check("has_many through a has_one hop", a1.scans.map(&:at).sort == ["bar", "door"] && b2.scans.to_a.empty?)
    check("count through a has_one hop", a1.scans.count == 2)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "venues", force: :cascade do |t|
    t.string "name"
    t.boolean "open", default: false
  end
  create_table "events", force: :cascade do |t|
    t.string "name"
    t.integer "venue_id"
  end
  create_table "posters", force: :cascade do |t|
    t.integer "event_id"
    t.string "title"
  end
  create_table "tickets", force: :cascade do |t|
    t.string "seat"
    t.integer "event_id"
  end
  create_table "gates", force: :cascade do |t|
    t.string "name"
  end
  create_table "stubs", force: :cascade do |t|
    t.integer "ticket_id"
    t.integer "gate_id"
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
fn a_through_over_a_singular_hop_reads_through_it() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_through_over_a_singular_hop_reads_through_it_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

// Shapes `check` refuses, written apart from `app()` so the Rails probe
// runner, which reads every `.write(path, r##"..."##)`, sees only the app.
const POLYMORPHIC_HOP: &str = r##"class Badge < ApplicationRecord
  belongs_to :holder, polymorphic: true
  has_one :venue, through: :holder
end
"##;

const COLLECTION_HOP: &str = r##"class Venue < ApplicationRecord
  has_many :events
  has_one :poster, through: :events
end
"##;

const SCOPED_SOURCES: &str = r##"class Event < ApplicationRecord
  belongs_to :venue, -> { where(open: true) }
  has_one :poster, -> { where(title: "tonight") }
  has_many :tickets, -> { where(seat: "a1") }
end
"##;

/// No join follows a polymorphic belongs_to hop, so the reader would
/// read a column the target does not have, and Rails refuses a has_one
/// :through over a collection: each an error, not a reader.
#[test]
fn a_through_no_join_follows_is_unsupported() {
    let (_emitted, errors) = app()
        .write("app/models/badge.rb", POLYMORPHIC_HOP)
        .write("app/models/venue.rb", COLLECTION_HOP)
        .emit(roundhouse::project::BuildTarget::Ruby);
    let refused: Vec<&String> = errors.iter().filter(|e| e.contains("through_chain")).collect();
    assert_eq!(refused.len(), 2, "{errors:?}");
    assert!(refused.iter().any(|e| e.contains("Badge.venue")), "{errors:?}");
    assert!(refused.iter().any(|e| e.contains("Venue.poster") && e.contains("collection")), "{errors:?}");
}

/// The reader applies the through's own scope and its hop's, not its
/// source's, which Rails applies: a scoped source is an error, not a
/// reader that answers the rows its scope leaves out.
#[test]
fn a_through_over_a_scoped_source_is_unsupported() {
    let (_emitted, errors) = app()
        .write("app/models/event.rb", SCOPED_SOURCES)
        .emit(roundhouse::project::BuildTarget::Ruby);
    let refused: Vec<&String> = errors.iter().filter(|e| e.contains("through_source_scope")).collect();
    for name in ["Ticket.venue", "Ticket.open_venue", "Ticket.poster", "Ticket.companions"] {
        assert!(refused.iter().any(|e| e.contains(name)), "{name}: {errors:?}");
    }
    assert_eq!(refused.len(), 4, "{errors:?}");
}

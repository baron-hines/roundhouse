//! A record and its clone share the has_one target either of them
//! loads, so the other reads it without a query, while a dup holds none
//! and a fresh find loads its own, as Rails' clone keeps the original's
//! association cache. Ignored as a fork spec until the compiler does;
//! the expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/house.rb", r##"class House < ApplicationRecord
  has_one :mailbox
end
"##)
        .write("app/models/mailbox.rb", r##"class Mailbox < ApplicationRecord
  belongs_to :house, optional: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.queries
    count = 0
    counter = ->(event) { count += 1 unless event.payload[:name] == "SCHEMA" }
    ActiveSupport::Notifications.subscribed(counter, "sql.active_record") { yield }
    count
  end

  def self.run
    h = House.create!(name: "corner")
    Mailbox.create!(house_id: h.id, color: "red")
    original = House.find(h.id)
    copy = original.clone
    # Rails: 1, then 0 for the original and a later clone
    check("the clone loads", queries { copy.mailbox.color } == 1)
    check("the original reads the clone's load", queries { original.mailbox.color } == 0)
    check("a later clone reads it too", queries { original.clone.mailbox.color } == 0)
    check("a dup holds none", original.dup.mailbox.nil?)
    check("a fresh find loads its own", queries { House.find(h.id).mailbox.color } == 2)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "houses", force: :cascade do |t|
    t.string "name"
  end
  create_table "mailboxes", force: :cascade do |t|
    t.integer "house_id"
    t.string "color"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: persistence"]
fn a_record_and_its_clone_share_a_has_one() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: persistence"]
fn a_record_and_its_clone_share_a_has_one_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

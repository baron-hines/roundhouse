//! `update_all` and `delete_all` write only the rows of the relation's
//! window, its ORDER BY, LIMIT and OFFSET, as Rails' `compile_update` and
//! `compile_delete` do: on SQLite the rows go by key through a subquery.
//! The runtime wrote `UPDATE t SET ... WHERE ...` and dropped the window,
//! so every row matched.
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/inbox.rb", r##"class Inbox < ApplicationRecord
  has_many :messages
end
"##)
        .write("app/models/message.rb", r##"class Message < ApplicationRecord
  belongs_to :inbox
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, actual, expected)
    raise "#{label}: #{actual.inspect}, Rails answers #{expected.inspect}" unless actual == expected
  end

  def self.run
    inbox = Inbox.create!(name: "work")
    (1..7).each { |i| Message.create!(inbox: inbox, subject: "m#{i}", state: 0) }
    check("limit", [inbox.messages.limit(2).update_all(state: 1), Message.where(state: 1).count], [2, 2])
    check("order and limit", [Message.order(id: :desc).limit(3).update_all(state: 2), Message.where(state: 2).order(:id).pluck(:subject)], [3, ["m5", "m6", "m7"]])
    check("offset", [Message.order(:id).limit(1).offset(2).update_all(state: 3), Message.where(state: 3).pluck(:subject)], [1, ["m3"]])
    check("joined", [Message.joins(:inbox).where(inboxes: { name: "work" }).order(:id).limit(1).update_all(state: 4), Message.where(state: 4).pluck(:subject)], [1, ["m1"]])
    check("delete", [Message.order(:id).limit(3).delete_all, Message.order(:id).pluck(:subject)], [3, ["m4", "m5", "m6", "m7"]])
    check("no window", [Message.where(state: 0).update_all(state: 5), Message.where(state: 5).pluck(:subject)], [1, ["m4"]])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "inboxes", force: :cascade do |t|
    t.string "name"
  end
  create_table "messages", force: :cascade do |t|
    t.string "subject"
    t.integer "inbox_id"
    t.integer "state"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn update_all_and_delete_all_keep_the_relations_window() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn update_all_and_delete_all_keep_the_relations_window_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

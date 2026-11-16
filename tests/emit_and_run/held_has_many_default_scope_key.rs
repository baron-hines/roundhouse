//! A has_many the app creates on (so its owner holds what it creates)
//! still reads past a target default scope on the foreign key, as an
//! association that holds nothing does: Rails' association condition on
//! the key stands in for the default scope's. The probe's expected values
//! are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/current.rb", r##"class Current < ActiveSupport::CurrentAttributes
  attribute :club #: Club
end
"##)
        .write("app/models/club.rb", r##"class Club < ApplicationRecord
  has_many :members
end
"##)
        .write("app/models/member.rb", r##"class Member < ApplicationRecord
  belongs_to :club, optional: true
  default_scope { where(club_id: Current.club.id) if Current.club }
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    chess = Club.create!(name: "chess")
    rowing = Club.create!(name: "rowing")
    chess.members.create!(name: "ada")
    rowing.members.create!(name: "bo")
    Current.club = rowing
    # Rails: ["ada"], the key's condition in place of the default scope's
    check("held reader", Club.find(chess.id).members.map(&:name) == ["ada"])
    check("held owner", chess.members.map(&:name) == ["ada"])
    Current.reset
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "clubs", force: :cascade do |t|
    t.string "name"
  end
  create_table "members", force: :cascade do |t|
    t.integer "club_id"
    t.string "name"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_held_has_many_reads_past_a_default_scope_on_its_key() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_held_has_many_reads_past_a_default_scope_on_its_key_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

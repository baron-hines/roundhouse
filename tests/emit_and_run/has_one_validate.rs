//! A `has_one ..., validate: true` validates its loaded target when that
//! record is new or changed, and the owner takes `<name> is invalid`
//! (Rails 8.1.4's `validate_single_association`); a has_one without the
//! option doesn't (`ingest::has_one_write`). The probe's expected values
//! are ActiveRecord 8.1.4's answers.

use super::emit_and_run;
use roundhouse::build_target::BuildTarget;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/author.rb", r##"class Author < ApplicationRecord
  has_one :profile
  has_one :checked_profile, class_name: "Profile", validate: true
end
"##)
        .write("app/models/profile.rb", r##"class Profile < ApplicationRecord
  belongs_to :author
  validates :bio, presence: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.reset
    [Profile, Author].each(&:delete_all)
  end

  # Rails: [false, ["Checked profile is invalid"]]
  def self.has_one_validate_true
    a = Author.new(name: "n"); a.checked_profile = Profile.new
    [a.valid?, a.errors.full_messages]
  end

  # Rails: [true, [], true]
  def self.has_one_validates_held_record
    a = Author.create!(name: "a"); a.profile = Profile.new(bio: "ok") rescue nil
    a2 = Author.new(name: "n"); a2.profile = Profile.new
    [a2.valid?, a2.errors.full_messages, a2.save]
  end

  def self.run
    reset
    check("has_one_validate_true", has_one_validate_true == [false, ["Checked profile is invalid"]])
    reset
    check("has_one_validates_held_record", has_one_validates_held_record == [true, [], true])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "authors", force: :cascade do |t|
    t.string "name"
    t.boolean "active", default: true
  end
  create_table "profiles", force: :cascade do |t|
    t.integer "author_id"
    t.string "bio"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_has_one_validate_true_validates_its_target_as_rails_does() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_has_one_validate_true_validates_its_target_as_rails_does_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
fn a_strict_target_keeps_validate_true_the_warned_gap() {
    let warned = |target| {
        app().lowering_warnings(target).iter().any(|d| d.message.contains("has_one :checked_profile, validate:"))
    };
    assert!(warned(BuildTarget::Rust), "the Rust target doesn't validate the target, so it warns");
    assert!(!warned(BuildTarget::Ruby), "the Ruby family validates the target");
}

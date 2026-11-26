//! A has_one's writer from app code (`author.profile = record`), as Rails
//! 8.1.4's `HasOneAssociation#replace` runs it: on a persisted owner the
//! record gets the key, the inverse and the `as:` type (the base class's
//! name for an STI owner) and is saved at once, raising `RecordNotSaved`
//! when it is invalid; the old record is removed per `dependent:` (nulled
//! and saved without one, raising `RecordNotSaved` when that save fails). A new owner holds the record, inverse set, and
//! saves it with the key after itself (`ingest::has_one_write`). The probe
//! passes unchanged on ActiveRecord 8.1.4.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/author.rb", r##"class Author < ApplicationRecord
  has_one :profile
  has_one :image, as: :imageable
  has_one :badge, dependent: :destroy
  has_one :card
  has_one :stamp, dependent: :delete
end
"##)
        .write("app/models/badge.rb", r##"class Badge < ApplicationRecord
  belongs_to :author
  validate :label_allowed

  def label_allowed
    errors.add(:label, "is not allowed") if label == "bad"
  end
end
"##)
        .write("app/models/stamp.rb", r##"class Stamp < ApplicationRecord
  belongs_to :author
  validate :label_allowed

  def label_allowed
    errors.add(:label, "is not allowed") if label == "bad"
  end
end
"##)
        .write("app/models/card.rb", r##"class Card < ApplicationRecord
  belongs_to :author
end
"##)
        .write("app/models/editor.rb", r##"class Editor < Author
end
"##)
        .write("app/models/image.rb", r##"class Image < ApplicationRecord
  belongs_to :imageable, polymorphic: true
end
"##)
        .write("app/models/profile.rb", r##"class Profile < ApplicationRecord
  belongs_to :author, optional: true
  validates :bio, presence: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    # A persisted owner: the record gets the key and the inverse, and is saved.
    a = Author.create!(name: "a")
    p = Profile.new(bio: "b")
    a.profile = p
    check("inverse", p.author.equal?(a))
    check("key", p.author_id == a.id)
    check("saved", p.persisted?)
    # Replacing it nulls the old record's key (no dependent:) and saves it.
    q = Profile.new(bio: "c")
    a.profile = q
    check("old key nulled", Profile.find(p.id).author_id.nil?)
    check("new one read", Author.find(a.id).profile.bio == "c")
    # dependent: :destroy destroys the old record.
    a.badge = Badge.new
    first = a.badge.id
    a.badge = Badge.new
    check("old badge destroyed", !Badge.exists?(first))
    check("one badge", Badge.count == 1)
    # as: writes the type, the base class's name for an STI owner.
    i = Image.new
    a.image = i
    check("as type", i.imageable_type == "Author")
    check("as key", i.imageable_id == a.id && i.persisted?)
    e = Editor.create!(name: "e")
    j = Image.new
    e.image = j
    check("sti base type", j.imageable_type == "Author")
    # A new owner holds the record and saves it, with the key, after itself.
    n = Author.new(name: "n")
    r = Profile.new(bio: "held")
    n.profile = r
    check("held unsaved", !r.persisted?)
    check("held inverse", r.author.equal?(n))
    check("owner saves", n.save)
    check("held saved with key", r.persisted? && Profile.find(r.id).author_id == n.id)
    # A persisted owner's invalid record isn't saved: RecordNotSaved.
    raised = false
    begin
      a.profile = Profile.new
    rescue ActiveRecord::RecordNotSaved
      raised = true
    end
    check("record not saved", raised)
    # The failure rolls the removal back: the old record keeps its key and
    # stays the target, and a destroyed one stays in the table.
    check("old profile kept", Profile.find(q.id).author_id == a.id && q.author_id == a.id && a.profile.equal?(q))
    kept = a.badge
    raised = false
    begin
      a.badge = Badge.new(label: "bad")
    rescue StandardError
      # Rails 8.1.4 raises FrozenError here: its failure path writes the
      # key back onto the destroyed record. Either way the replacement
      # is rolled back.
      raised = true
    end
    check("old badge kept", raised && Badge.exists?(kept.id) && a.badge.equal?(kept))
    # dependent: :delete: the row is back and the target kept, but the
    # record stays destroyed in memory, as in Rails (its delete isn't
    # enlisted in the transaction).
    a.stamp = Stamp.new
    gone = a.stamp
    raised = false
    begin
      a.stamp = Stamp.new(label: "bad")
    rescue StandardError
      raised = true
    end
    check("old stamp kept", raised && Stamp.exists?(gone.id) && a.stamp.equal?(gone) && gone.destroyed? && !gone.persisted? && gone.author_id == a.id)
    # A required belongs_to can't take the nulled key: replacing raises,
    # and the old record keeps its key.
    a.card = Card.new
    c = a.card
    raised = false
    begin
      a.card = Card.new
    rescue ActiveRecord::RecordNotSaved
      raised = true
    end
    check("old required raises", raised)
    check("old required kept", Card.find(c.id).author_id == a.id && c.author_id == a.id)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "authors", force: :cascade do |t|
    t.string "name"
    t.string "type"
  end
  create_table "profiles", force: :cascade do |t|
    t.integer "author_id"
    t.string "bio"
  end
  create_table "images", force: :cascade do |t|
    t.string "imageable_type"
    t.integer "imageable_id"
  end
  create_table "badges", force: :cascade do |t|
    t.integer "author_id"
    t.string "label"
  end
  create_table "stamps", force: :cascade do |t|
    t.integer "author_id"
    t.string "label"
  end
  create_table "cards", force: :cascade do |t|
    t.integer "author_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_has_one_writer_replaces_as_rails_does() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_has_one_writer_replaces_as_rails_does_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

/// A has_one that only `db/seeds.rb` assigns gets the writer too.
fn seeded_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/author.rb", "class Author < ApplicationRecord\n  has_one :card\nend\n")
        .write("app/models/card.rb", "class Card < ApplicationRecord\n  belongs_to :author\nend\n")
        .write("db/seeds.rb", "author = Author.create!(name: \"s\")\nauthor.card = Card.new\n")
        .write("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table \"authors\", force: :cascade do |t|\n    t.string \"name\"\n  end\n  create_table \"cards\", force: :cascade do |t|\n    t.integer \"author_id\"\n  end\nend\n")
}

const SEEDED: &str = "require File.expand_path(\"db/seeds\", Dir.pwd)\nSeeds.run\nraise \"card not saved\" unless Card.count == 1 && Card.first.author_id == Author.first.id\nputs \"seeded\"\n";

#[test]
fn a_has_one_only_seeds_assign_gets_the_writer() {
    let run = seeded_app().run_ruby(SEEDED);
    run.assert_passes();
    assert!(run.stdout.contains("seeded"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_has_one_only_seeds_assign_gets_the_writer_on_spinel() {
    let run = seeded_app().run_spinel_with_db(SEEDED);
    run.assert_passes();
    assert!(run.stdout.contains("seeded"), "{}", run.stdout);
}

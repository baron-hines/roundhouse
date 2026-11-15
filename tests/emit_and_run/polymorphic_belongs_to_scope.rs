//! A polymorphic `belongs_to`'s scope narrows what its reader finds, on
//! whichever class the type column names, as Rails builds the reader's
//! query from the association scope: a tag whose photo is hidden reads
//! nil, read lazily or preloaded. The expected values are ActiveRecord
//! 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/photo.rb", r##"class Photo < ApplicationRecord
  has_many :tags, as: :taggable
end
"##)
        .write("app/models/album.rb", r##"class Album < ApplicationRecord
  has_many :tags, as: :taggable
end
"##)
        .write("app/models/tag.rb", r##"class Tag < ApplicationRecord
  belongs_to :taggable, -> { where(visible: true) }, polymorphic: true, optional: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    shown = Photo.create!(title: "harbor")
    hidden = Photo.create!(title: "draft", visible: false)
    album = Album.create!(title: "spring")
    Tag.create!(label: "p", taggable: shown)
    Tag.create!(label: "q", taggable: hidden)
    Tag.create!(label: "r", taggable: album)
    Tag.create!(label: "s", taggable_type: "Album", taggable_id: album.id + 100)
    # Rails: ["Photo", "Photo", "Album", "Album"]
    check("stored", Tag.order(:label).map { |t| t.taggable_type } == ["Photo", "Photo", "Album", "Album"])
    # Rails: ["harbor", nil, "spring", nil]
    check("reader", Tag.order(:label).map { |t| t.taggable && t.taggable.title } == ["harbor", nil, "spring", nil])
    check("preload", Tag.order(:label).preload(:taggable).map { |t| t.taggable && t.taggable.title } == ["harbor", nil, "spring", nil])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "photos", force: :cascade do |t|
    t.string "title"
    t.boolean "visible", default: true
  end
  create_table "albums", force: :cascade do |t|
    t.string "title"
    t.boolean "visible", default: true
  end
  create_table "tags", force: :cascade do |t|
    t.string "label"
    t.string "taggable_type"
    t.integer "taggable_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_polymorphic_belongs_to_reads_through_its_scope() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_polymorphic_belongs_to_reads_through_its_scope_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

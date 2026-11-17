//! An invalid child built on an unloaded nested-attributes has_many puts
//! its own errors on the owner, per attribute (`Jars label can't be
//! blank`), as Rails' autosave validation copies them; a plain has_many
//! answers `Jars is invalid`. The owner validates only the records the
//! collection holds and loads nothing (that half passes on `main`).
//! Ignored as a fork spec until the compiler does; the expected values
//! are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/shelf.rb", r##"class Shelf < ApplicationRecord
  has_many :jars
  accepts_nested_attributes_for :jars
end
"##)
        .write("app/models/jar.rb", r##"class Jar < ApplicationRecord
  belongs_to :shelf
  validates :label, presence: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    shelf = Shelf.create!(name: "pantry")
    stale = Jar.create!(shelf: shelf, label: "old")
    stale.update_columns(label: "")
    shelf = Shelf.find(shelf.id)
    shelf.jars.build(label: "honey")
    check("the held jar is valid, the row is not read", shelf.valid?)
    check("validation loads nothing", !shelf.jars.loaded?)
    shelf.jars.build(label: "")
    check("a held invalid jar fails the owner", !shelf.valid?)
    check("its error", shelf.errors.full_messages == ["Jars label can't be blank"])
    check("still unloaded", !shelf.jars.loaded?)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "shelves", force: :cascade do |t|
    t.string "name"
  end
  create_table "jars", force: :cascade do |t|
    t.integer "shelf_id"
    t.string "label"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: nested attributes"]
fn nested_built_child_errors_reach_the_owner() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: nested attributes"]
fn nested_built_child_errors_reach_the_owner_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

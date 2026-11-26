//! An unsaved record's `id` is `nil`, so `id.present?` is false and
//! `id.blank?` true until it saves, as Rails answers for its nil key.
//! Ignored as a fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/note.rb", r##"class Note < ApplicationRecord
  def stored?
    id.present?
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    n = Note.new(body: "draft")
    # Rails: false, true, false, nil
    check("unsaved id.present?", !n.id.present?)
    check("unsaved id.blank?", n.id.blank?)
    check("unsaved stored?", !n.stored?)
    check("unsaved id.presence", n.id.presence.nil?)
    n.save!
    # Rails: true, true
    check("saved id.present?", n.id.present? && n.stored?)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "notes", force: :cascade do |t|
    t.string "body"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: ruby semantics"]
fn an_unsaved_record_id_is_not_present() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn an_unsaved_record_id_is_not_present_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

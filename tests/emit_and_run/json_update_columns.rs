//! `update_columns` of a json column changed in place forgets the change:
//! the written value becomes the original, so the record is not
//! `changed?` and a later `update!` doesn't write it again;
//! `clear_attribute_changes` forgets an in-place change the same way, as
//! Rails' `forget_change` does. Ignored as a fork spec until the compiler
//! does; the expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/profile.rb", r##"class Profile < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    p = Profile.create!(name: "a", settings: { "theme" => "light" })
    p = Profile.find(p.id)
    p.settings["theme"] = "dark"
    check("a mutation in place is a change", p.changed?)
    p.update_columns(settings: p.settings)
    check("update_columns forgets it", !p.changed?)
    check("the row holds it", Profile.find(p.id).settings == { "theme" => "dark" })
    p.update!(name: "b")
    check("a later update! writes only name", p.saved_changes.keys == ["name"])
    p.settings["theme"] = "plain"
    p.clear_attribute_changes(["settings"])
    check("clear_attribute_changes forgets an in-place change", !p.changed?)
    p.update!(name: "c")
    check("and the save leaves the row", Profile.find(p.id).settings == { "theme" => "dark" })
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "profiles", force: :cascade do |t|
    t.string "name"
    t.json "settings"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: attribute types"]
fn update_columns_forgets_an_in_place_change() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: attribute types"]
fn update_columns_forgets_an_in_place_change_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

//! Writing a column a partial select left out is a change whatever it
//! writes, nil included, so the save's UPDATE sets it, as Rails treats an
//! unloaded attribute as holding no original value. Ignored as a fork
//! spec until the compiler does; the expected values are ActiveRecord
//! 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/draft.rb", r##"class Draft < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    s = Draft.create!(name: "a", revision: 7)
    partial = Draft.select(:id, :name).find(s.id)
    partial[:revision] = nil
    # Rails: true
    check("the write is a change", partial.changed? == true)
    partial.save!
    # Rails: nil
    check("the UPDATE sets it", Draft.find(s.id).revision.nil?)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "drafts", force: :cascade do |t|
    t.string "name"
    t.integer "revision"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: persistence"]
fn writing_a_column_a_partial_select_left_out_is_saved() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: persistence"]
fn writing_a_column_a_partial_select_left_out_is_saved_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

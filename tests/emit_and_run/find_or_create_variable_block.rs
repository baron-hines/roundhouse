//! `Origin.find_or_create_by(identifier: identifier) { |o| o.note ||= "from block" }` (lobsters' `Domain`): with a
//! variable condition and no initialization callback on the model, the block runs on the record a miss builds, after
//! its attributes and before its save, as Rails' `create(attributes, &block)` does; a hit returns the row untouched.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write(
            "app/models/origin.rb",
            "class Origin < ApplicationRecord\n  validates :note, presence: true\n\n  def self.claim(identifier)\n    Origin.find_or_create_by(identifier: identifier) { |o| o.note ||= \"from block\" }\n  end\nend\n",
        )
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema[8.1].define(version: 2026_01_01_000000) do\n  create_table \"origins\", force: :cascade do |t|\n    t.string \"identifier\"\n    t.string \"note\"\n  end\nend\n",
        )
}

const SCRIPT: &str = r##"made = Origin.claim("example.com")
raise "a miss saves the block's record: #{made.note.inspect} #{made.persisted?}" unless made.persisted? && made.note == "from block" && made.identifier == "example.com"
Origin.where(identifier: "example.com").update_all(note: "kept")
again = Origin.claim("example.com")
raise "a hit is the stored row: #{again.note.inspect}" unless again.id == made.id && again.note == "kept"
raise "one row: #{Origin.count}" unless Origin.count == 1
puts "variable block passed"
"##;

#[test]
fn a_variable_condition_find_or_create_runs_its_block_on_a_miss() {
    app().run_ruby(SCRIPT).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_variable_condition_find_or_create_runs_its_block_on_a_miss_on_spinel() {
    app().run_spinel_with_db(SCRIPT).assert_passes();
}

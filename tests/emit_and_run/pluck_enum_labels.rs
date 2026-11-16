//! `pluck` and `pick` of an enum column answer its labels, beside the
//! plain value of another column, as Rails' EnumType deserializes them.
//! Ignored as a fork spec until the compiler does; the expected values
//! are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/lamp.rb", r##"class Lamp < ApplicationRecord
  enum :mode, { off: 0, lit: 3 }
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    Lamp.create!(mode: :off, watts: 10)
    lit = Lamp.create!(mode: :lit, watts: 50)
    # Rails: ["off", "lit"], [["off", 10], ["lit", 50]], ["lit", 50]
    check("pluck of the enum", Lamp.order(:id).pluck(:mode) == ["off", "lit"])
    check("pluck of a pair", Lamp.order(:id).pluck(:mode, :watts) == [["off", 10], ["lit", 50]])
    check("pick of a pair", Lamp.where(id: lit.id).pick(:mode, :watts) == ["lit", 50])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "lamps", force: :cascade do |t|
    t.integer "mode"
    t.integer "watts"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: relation queries"]
fn pluck_and_pick_of_an_enum_answer_its_labels() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: relation queries"]
fn pluck_and_pick_of_an_enum_answer_its_labels_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

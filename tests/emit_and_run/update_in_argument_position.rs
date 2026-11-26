//! `record.update(attrs)` used as a value, inside an argument list, a
//! comparison or an array literal, assigns the attributes, saves, and
//! answers whether the save succeeded, as Rails does. The probe's
//! expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/kettle.rb", r##"class Kettle < ApplicationRecord
  validates :brand, presence: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.saved?(answer)
    answer == true
  end

  # Rails: true, "steel"
  def self.update_as_an_argument
    k = Kettle.create!(brand: "tin")
    check("update_as_an_argument", k.update(brand: "steel") == true)
    check("update_as_an_argument reloaded", Kettle.find(k.id).brand == "steel")
  end

  # Rails: [false, "tin", true, 4]
  def self.update_in_an_array
    k = Kettle.create!(brand: "tin")
    pair = [k.update(brand: ""), Kettle.find(k.id).brand]
    pair + [saved?(k.update(brand: "iron", litres: 4)), Kettle.find(k.id).litres]
  end

  def self.run
    Kettle.delete_all
    update_as_an_argument
    check("update_in_an_array", update_in_an_array == [false, "tin", true, 4])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "kettles", force: :cascade do |t|
    t.string "brand"
    t.integer "litres", default: 1
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn update_as_a_value_saves_and_answers() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn update_as_a_value_saves_and_answers_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

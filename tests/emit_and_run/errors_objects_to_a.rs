//! `errors.to_a` is the full messages and `errors.objects` the
//! `ActiveModel::Error`s, both in the order they were added. The probe's
//! expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/recipe.rb", r##"class Recipe < ApplicationRecord
  validates :dish, presence: true
  validates :serves, numericality: { only_integer: true }
  validate :oven_note

  def oven_note
    errors.add(:base, "Oven temperature is missing") if dish == "bread"
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  # Rails: ["Dish can't be blank", "Serves is not a number"]
  def self.to_a_lists_full_messages
    r = Recipe.new(dish: "", serves: nil)
    r.valid?
    r.errors.to_a
  end

  # Rails: [["Oven temperature is missing"], "Oven temperature is missing"]
  def self.to_a_keeps_base_messages_bare
    r = Recipe.new(dish: "bread", serves: 4)
    r.valid?
    [r.errors.to_a, r.errors.to_a.last]
  end

  # Rails: [[:dish, :serves], ["can't be blank", "is not a number"], 2]
  def self.objects_are_the_errors
    r = Recipe.new(dish: "", serves: nil)
    r.valid?
    objects = r.errors.objects
    [objects.map(&:attribute), objects.map(&:message), objects.size]
  end

  # Rails: [[], []]
  def self.a_valid_record_has_none
    r = Recipe.new(dish: "soup", serves: 2)
    r.valid?
    [r.errors.to_a, r.errors.objects]
  end

  def self.run
    check("to_a_lists_full_messages", to_a_lists_full_messages == ["Dish can't be blank", "Serves is not a number"])
    check("to_a_keeps_base_messages_bare", to_a_keeps_base_messages_bare == [["Oven temperature is missing"], "Oven temperature is missing"])
    check("objects_are_the_errors", objects_are_the_errors == [[:dish, :serves], ["can't be blank", "is not a number"], 2])
    check("a_valid_record_has_none", a_valid_record_has_none == [[], []])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "recipes", force: :cascade do |t|
    t.string "dish"
    t.integer "serves"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn errors_to_a_and_objects_answer_as_rails_does() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn errors_to_a_and_objects_answer_as_rails_does_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

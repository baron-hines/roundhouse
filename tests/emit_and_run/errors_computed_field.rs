//! `errors[field]`, `messages_for(field)` and `full_messages_for(field)`
//! with a field computed at run time (a Symbol or String held in a local
//! or a parameter), read as the literal field is. The probe's expected
//! values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/tent.rb", r##"class Tent < ApplicationRecord
  validates :brand, presence: true
  validates :berths, numericality: { greater_than: 0 }

  def summary_for(field)
    errors.full_messages_for(field).join("; ")
  end
end
"##)
        .write("app/models/packing_list.rb", r##"class PackingList
  def errors
    @errors ||= { "stove" => ["is missing"] }
  end

  def literal_read
    errors["stove"]
  end

  def read(key)
    errors[key.to_s]
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.invalid_tent
    t = Tent.new(brand: "", berths: 0)
    t.valid?
    t
  end

  # Rails: [["can't be blank"], ["must be greater than 0"]]
  def self.index_by_each_field
    t = invalid_tent
    %i[brand berths].map { |field| t.errors[field] }
  end

  # Rails: [["Brand can't be blank"], ["Berths must be greater than 0"]]
  def self.full_messages_for_each_field
    t = invalid_tent
    %i[brand berths].map { |field| t.errors.full_messages_for(field) }
  end

  # Rails: [["must be greater than 0"], []]
  def self.messages_for_a_string_field
    t = invalid_tent
    ["berths", "colour"].map { |name| t.errors.messages_for(name) }
  end

  # Rails: ["Brand can't be blank", ""]
  def self.through_a_method_parameter
    t = invalid_tent
    [t.summary_for(:brand), t.summary_for(:colour)]
  end

  # Rails: [["is missing"], ["is missing"], nil]: an object's own errors Hash answers [] itself
  def self.own_errors_keep_their_index
    list = PackingList.new
    [list.literal_read, list.read(:stove), list.read(:tarp)]
  end

  def self.run
    check("index_by_each_field", index_by_each_field == [["can't be blank"], ["must be greater than 0"]])
    check("full_messages_for_each_field", full_messages_for_each_field == [["Brand can't be blank"], ["Berths must be greater than 0"]])
    check("messages_for_a_string_field", messages_for_a_string_field == [["must be greater than 0"], []])
    check("through_a_method_parameter", through_a_method_parameter == ["Brand can't be blank", ""])
    check("own_errors_keep_their_index", own_errors_keep_their_index == [["is missing"], ["is missing"], nil])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "tents", force: :cascade do |t|
    t.string "brand"
    t.integer "berths"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn errors_read_with_a_computed_field_answers_as_rails_does() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn errors_read_with_a_computed_field_answers_as_rails_does_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

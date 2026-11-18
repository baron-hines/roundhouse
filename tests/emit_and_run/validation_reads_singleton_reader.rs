//! A validation reads its attribute through the reader, as ActiveModel's
//! `read_attribute_for_validation` (`send(attr)`) does: a reader a record
//! answers from its singleton (as a test stub defines one) is what the
//! inclusion and presence checks see, not the column's value. Ignored as
//! a fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/printer.rb", r##"class Printer < ApplicationRecord
  validates :paper, inclusion: { in: %w[a4 letter] }
  validates :name, presence: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    plain = Printer.new(name: "hall", paper: "a4")
    check("an unstubbed record is valid", plain.valid?)
    odd = Printer.new(name: "hall", paper: "a4")
    odd.define_singleton_method(:paper) { "tabloid" }
    check("inclusion reads the stubbed reader", !odd.valid? && odd.errors[:paper] == ["is not included in the list"])
    unnamed = Printer.new(name: "hall", paper: "letter")
    unnamed.define_singleton_method(:name) { "" }
    check("presence reads the stubbed reader", !unnamed.valid? && unnamed.errors[:name] == ["can't be blank"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "printers", force: :cascade do |t|
    t.string "name"
    t.string "paper"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: validations"]
fn validation_reads_the_attribute_through_its_reader() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: validations"]
fn validation_reads_the_attribute_through_its_reader_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

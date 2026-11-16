//! `errors.delete(:field, type)` removes only the field's errors of that
//! type (a Symbol, or the message a String type was added with) and
//! answers their messages, or nil when none matched. The probe's expected
//! values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/locker.rb", r##"class Locker < ApplicationRecord
  validates :code, presence: true
  validates :code, length: { minimum: 4 }
  validate :code_digits

  def code_digits
    errors.add(:code, "must be digits only") if code && code !~ /\A\d*\z/
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  # Rails: [["can't be blank"], ["Code is too short (minimum is 4 characters)"]]
  def self.delete_by_symbol_type
    l = Locker.new(code: "")
    l.valid?
    [l.errors.delete(:code, :blank), l.errors.full_messages]
  end

  # Rails: [["must be digits only"], ["Code is too short (minimum is 4 characters)"], 1]
  def self.delete_by_message_type
    l = Locker.new(code: "ab")
    l.valid?
    [l.errors.delete(:code, "must be digits only"), l.errors.full_messages, l.errors.size]
  end

  # Rails: [nil, 2]
  def self.delete_of_an_absent_type_is_nil
    l = Locker.new(code: "ab")
    l.valid?
    [l.errors.delete(:code, :taken), l.errors.size]
  end

  # Rails: [nil, ["can't be blank", "is too short (minimum is 4 characters)"]]
  def self.delete_on_another_field_keeps_these
    l = Locker.new(code: "")
    l.valid?
    [l.errors.delete(:owner, :blank), l.errors[:code]]
  end

  def self.run
    check("delete_by_symbol_type", delete_by_symbol_type == [["can't be blank"], ["Code is too short (minimum is 4 characters)"]])
    check("delete_by_message_type", delete_by_message_type == [["must be digits only"], ["Code is too short (minimum is 4 characters)"], 1])
    check("delete_of_an_absent_type_is_nil", delete_of_an_absent_type_is_nil == [nil, 2])
    check("delete_on_another_field_keeps_these", delete_on_another_field_keeps_these == [nil, ["can't be blank", "is too short (minimum is 4 characters)"]])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "lockers", force: :cascade do |t|
    t.string "code"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn errors_delete_with_a_type_answers_as_rails_does() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn errors_delete_with_a_type_answers_as_rails_does_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

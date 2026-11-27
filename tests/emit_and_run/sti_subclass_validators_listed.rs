//! An STI subclass's `validates_with` and key-named validators
//! (`validates :body, length: {...}`) are built when its body runs and
//! listed in the subclass's `validators` and `validators_on`, not the
//! base's, and the subclass runs them, as Rails' validates_with does for
//! the class that declares it. Ignored as a fork spec until the compiler
//! does; the expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/validators/no_shouting_validator.rb", r##"class NoShoutingValidator < ActiveModel::Validator
  def validate(record)
    record.errors.add(:body, "is shouting") if record.body.to_s == record.body.to_s.upcase
  end
end
"##)
        .write("app/models/entry.rb", r##"class Entry < ApplicationRecord
  validates :body, presence: true
end
"##)
        .write("app/models/memo.rb", r##"class Memo < Entry
  validates :body, length: { maximum: 12 }
  validates_with NoShoutingValidator
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    kinds = Memo.validators_on(:body).map { |v| v.class.name }
    check("the subclass lists its length validator", kinds.include?("ActiveRecord::Validations::LengthValidator"))
    check("the subclass lists the base's presence validator", kinds.include?("ActiveRecord::Validations::PresenceValidator"))
    check("the subclass lists its validates_with once", Memo.validators.grep(NoShoutingValidator).size == 1)
    check("the base lists neither", Entry.validators.map { |v| v.class.name } == ["ActiveRecord::Validations::PresenceValidator"])
    check("a quiet short memo is valid", Memo.new(body: "quiet note").valid?)
    loud = Memo.new(body: "LOUD")
    check("the validates_with runs", !loud.valid? && loud.errors[:body] == ["is shouting"])
    long = Memo.new(body: "a rather long note")
    check("the length validator runs", !long.valid? && long.errors[:body] == ["is too long (maximum is 12 characters)"])
    check("the base runs neither", Entry.new(body: "LOUD AND RATHER LONG").valid?)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "entries", force: :cascade do |t|
    t.string "type"
    t.string "body"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: validations"]
fn sti_subclass_lists_and_runs_its_validators() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: validations"]
fn sti_subclass_lists_and_runs_its_validators_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

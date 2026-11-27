//! A validator named by its key (`validates :title, word_count: {...}`,
//! the app's WordCountValidator) is built once, when the class body runs,
//! listed in `validators` and `validators_on`, and run under its `if:`,
//! as Rails turns the key into `validates_with WordCountValidator`.
//! Ignored as a fork spec until the compiler does; the expected values
//! are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/validators/word_count_validator.rb", r##"class WordCountValidator < ActiveModel::EachValidator
  @@built = 0

  def self.built
    @@built
  end

  def initialize(options)
    @@built += 1
    super
  end

  def validate_each(record, attribute, value)
    if value.to_s.split.size > options[:maximum]
      record.errors.add(attribute, "has too many words")
    end
  end
end
"##)
        .write("app/models/headline.rb", r##"class Headline < ApplicationRecord
  validates :title, word_count: { maximum: 3 }, if: :checked?
  validates :subtitle, word_count: { maximum: 5 }

  def checked?
    !draft
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    check("listed on title once", Headline.validators_on(:title).grep(WordCountValidator).size == 1)
    check("listed on subtitle once", Headline.validators_on(:subtitle).grep(WordCountValidator).size == 1)
    check("listed in validators", Headline.validators.grep(WordCountValidator).size == 2)
    check("its options are kept", Headline.validators_on(:title).first.options[:maximum] == 3)
    built = WordCountValidator.built
    check("built when the class loads", built == 2)
    long = Headline.new(title: "one two three four", subtitle: "short", draft: false)
    check("it runs", !long.valid? && long.errors[:title] == ["has too many words"])
    check("its if: guards it", Headline.new(title: "one two three four", subtitle: "short", draft: true).valid?)
    Headline.new(title: "ok", subtitle: "ok", draft: false).valid?
    check("validating builds nothing more", WordCountValidator.built == built)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "headlines", force: :cascade do |t|
    t.string "title"
    t.string "subtitle"
    t.boolean "draft", default: false
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: validations"]
fn key_named_validator_is_built_once_and_listed() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: validations"]
fn key_named_validator_is_built_once_and_listed_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

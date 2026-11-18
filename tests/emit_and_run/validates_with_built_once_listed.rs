//! `validates_with SpellingValidator, words: [...]` builds its validator
//! once, when the class body runs, lists it in `validators`, and every
//! validation runs that one instance, as Rails' validates_with does.
//! Ignored as a fork spec until the compiler does; the expected values
//! are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/validators/spelling_validator.rb", r##"class SpellingValidator < ActiveModel::Validator
  @@built = 0

  def self.built
    @@built
  end

  attr_reader :runs

  def initialize(options)
    @@built += 1
    @runs = 0
    super
  end

  def validate(record)
    @runs += 1
    options[:words].each do |word|
      record.errors.add(:caption, "misspells #{word}") if record.caption.to_s.include?(word)
    end
  end
end
"##)
        .write("app/models/photo.rb", r##"class Photo < ApplicationRecord
  validates_with SpellingValidator, words: ["teh", "recieve"]
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    listed = Photo.validators.grep(SpellingValidator)
    check("listed once", listed.size == 1)
    check("built when the class loads", SpellingValidator.built == 1)
    bad = Photo.new(caption: "teh lake")
    check("it runs", !bad.valid? && bad.errors[:caption] == ["misspells teh"])
    Photo.new(caption: "the lake").valid?
    check("the listed instance ran both times", listed.first.runs == 2)
    check("validating builds nothing more", SpellingValidator.built == 1)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "photos", force: :cascade do |t|
    t.string "caption"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: validations"]
fn validates_with_builds_one_listed_validator() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: validations"]
fn validates_with_builds_one_listed_validator_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

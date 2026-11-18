//! `validates_with` passes the validator `class:` the class whose body
//! declares it: an STI subclass's own `validates_with` builds its
//! validator with the subclass, and a validator inherited from the base
//! was built with the base, as Rails' validates_with sets
//! `options[:class] = self`. Ignored as a fork spec until the compiler
//! does; the expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/reviewable.rb", r##"module Reviewable
  def reviewed_label
    "reviewed"
  end
end
"##)
        .write("app/validators/review_validator.rb", r##"class ReviewValidator < ActiveModel::Validator
  def initialize(options)
    @owner = options[:class]
    raise ArgumentError, "needs a Reviewable class" unless @owner.include?(Reviewable)
    super
  end

  def validate(record)
    record.errors.add(:base, "#{@owner.name} needs a reviewer") if record.reviewer.blank?
  end
end
"##)
        .write("app/validators/label_validator.rb", r##"class LabelValidator < ActiveModel::Validator
  def initialize(options)
    @owner = options[:class]
    super
  end

  def validate(record)
    record.errors.add(:url, "is blank for #{@owner.name}") if record.url.blank?
  end
end
"##)
        .write("app/models/link.rb", r##"class Link < ApplicationRecord
  validates_with LabelValidator
end
"##)
        .write("app/models/checked_link.rb", r##"class CheckedLink < Link
  include Reviewable
  validates_with ReviewValidator
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    link = CheckedLink.new(url: "")
    check("the subclass's validator was built with the subclass", !link.valid? && link.errors[:base] == ["CheckedLink needs a reviewer"])
    check("the inherited one with the base", link.errors[:url] == ["is blank for Link"])
    link.reviewer = "kim"
    link.url = "/a"
    check("and passes", link.valid?)
    check("the base does not run it", Link.new(url: "/b").valid?)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "links", force: :cascade do |t|
    t.string "type"
    t.string "url"
    t.string "reviewer"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: validations"]
fn validates_with_passes_the_declaring_class() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: validations"]
fn validates_with_passes_the_declaring_class_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

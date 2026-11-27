//! ActiveSupport's `String#starts_with?` and `ends_with?` answer as the
//! `start_with?` and `end_with?` they alias. `check` typed them as those
//! predicates while no runtime defined them, so the emitted call raised
//! NoMethodError. The expected values are ActiveRecord 8.1.4's answers,
//! with `active_support/all` loaded as a Rails app loads it.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/badge.rb", r##"class Badge < ApplicationRecord
  def question?
    label.starts_with?("Ask")
  end

  def plain?
    !label.ends_with?("!", "?")
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    asked = Badge.new(label: "Ask me")
    loud = Badge.new(label: "Hey!")
    # Rails: true, false, true, false
    check("starts_with?", asked.question?)
    check("starts_with? false", !loud.question?)
    check("ends_with? of several", asked.plain?)
    check("ends_with? true", !loud.plain?)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "badges", force: :cascade do |t|
    t.string "label"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn string_starts_with_alias_answers_as_start_with() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn string_starts_with_alias_answers_as_start_with_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

//! A concern prepended to a model: its `prepended` block runs on the
//! model, its `class_methods` land there, and its instance methods sit
//! ahead of the model's own, reaching them through `super`.
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/concerns/trimmed.rb", r##"module Trimmed
  extend ActiveSupport::Concern

  prepended do
    before_save :trim_label
  end

  class_methods do
    def trimmed_labels
      order(:label).pluck(:label)
    end
  end

  def display_label
    "[#{super}]"
  end

  private

  def trim_label
    self.label = label.strip
  end
end
"##)
        .write("app/models/badge.rb", r##"class Badge < ApplicationRecord
  prepend Trimmed

  def display_label
    label.upcase
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  # Rails: [["blue", "red"], "[RED]"]
  def self.prepended_concern
    Badge.create!(label: "  red ")
    Badge.create!(label: "blue  ")
    [Badge.trimmed_labels, Badge.order(:label).last.display_label]
  end

  def self.run
    Badge.delete_all
    check("prepended_concern", prepended_concern == [["blue", "red"], "[RED]"])
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
fn a_prepended_concern_runs_its_block_and_wraps_the_model() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_prepended_concern_runs_its_block_and_wraps_the_model_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

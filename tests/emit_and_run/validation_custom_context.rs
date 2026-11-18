//! A custom validation context runs only where it is asked for:
//! `valid?(:publish)` and `save(context: :publish)` run the `on: :publish`
//! rules beside the unconditional ones, a plain `valid?` or `save` skips
//! them, and `validation_context` answers the context inside a
//! validation, as ActiveModel's validation contexts do. Ignored as a fork
//! spec until the compiler does; the expected values are ActiveRecord
//! 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/essay.rb", r##"class Essay < ApplicationRecord
  validates :title, presence: true
  validates :summary, presence: true, on: :publish
  validate :seen_context

  def seen_context
    errors.add(:base, "checked for #{validation_context}") if validation_context == :publish && summary.blank?
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    e = Essay.new(title: "Rivers", summary: "")
    check("a plain valid? skips the context's rules", e.valid?)
    check("valid?(:publish) runs them", !e.valid?(:publish))
    check("with the validation's context", e.errors.full_messages == ["Summary can't be blank", "checked for publish"])
    check("save(context:) refuses", e.save(context: :publish) == false && !e.persisted?)
    check("a plain save passes", e.save == true)
    e.summary = "Water."
    check("save(context:) passes once filled", e.save(context: :publish) == true)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "essays", force: :cascade do |t|
    t.string "title"
    t.string "summary"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: validations"]
fn custom_validation_context_runs_where_asked() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: validations"]
fn custom_validation_context_runs_where_asked_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

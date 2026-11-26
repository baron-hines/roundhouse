//! A `has_one ..., autosave: true` with no `validate:` validates the
//! record it holds with its owner, and `validate: false` beside it turns
//! that off, as Rails' AutosaveAssociation reads `reflection.validate?`
//! (`has_one_validate` pins `validate: true`). Ignored as a fork spec
//! until the compiler does; the expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/quote.rb", r##"class Quote < ApplicationRecord
  has_one :delivery, autosave: true
  has_one :loose_delivery, class_name: "Delivery", autosave: true, validate: false
end
"##)
        .write("app/models/delivery.rb", r##"class Delivery < ApplicationRecord
  belongs_to :quote, optional: true
  validates :title, presence: true, length: { maximum: 10 }
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    q = Quote.new
    q.build_delivery(title: "")
    check("autosave validates the held record", !q.valid?)
    check("its error", q.errors.full_messages == ["Delivery title can't be blank"])
    q.delivery.title = "a title far too long"
    check("a changed held record is checked again", !q.valid? && q.errors.full_messages == ["Delivery title is too long (maximum is 10 characters)"])
    q.delivery.title = "ground"
    check("a valid one passes", q.valid?)
    loose = Quote.new
    loose.build_loose_delivery(title: "")
    check("validate: false does not check it", loose.valid?)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "quotes", force: :cascade do |t|
    t.string "ref"
  end
  create_table "deliveries", force: :cascade do |t|
    t.integer "quote_id"
    t.string "title"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: nested attributes"]
fn has_one_autosave_validates_its_record() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: nested attributes"]
fn has_one_autosave_validates_its_record_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

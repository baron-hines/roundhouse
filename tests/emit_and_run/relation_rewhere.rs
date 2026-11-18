//! `rewhere(hash)` drops the conditions on the hash's columns, keeps the
//! rest and adds the hash; `rewhere(nil)` drops every condition, as
//! Rails' QueryMethods#rewhere does. Ignored as a fork spec until the
//! compiler does; the expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/marble.rb", r##"class Marble < ApplicationRecord
  scope :red, -> { where(color: "red") }
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    Marble.create!(color: "red", size: 1)
    small_blue = Marble.create!(color: "blue", size: 1)
    Marble.create!(color: "blue", size: 2)
    # Rails: the small blue marble only
    swapped = Marble.where(color: "red", size: 1).rewhere(color: "blue")
    check("replaced and kept", swapped.pluck(:id) == [small_blue.id])
    check("a scope's condition replaced", Marble.red.rewhere(color: "blue").count == 2)
    check("rewhere(nil) drops every condition", Marble.red.where(size: 1).rewhere(nil).count == 3)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "marbles", force: :cascade do |t|
    t.string "color"
    t.integer "size"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: relation queries"]
fn rewhere_replaces_the_conditions_on_its_columns() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: relation queries"]
fn rewhere_replaces_the_conditions_on_its_columns_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

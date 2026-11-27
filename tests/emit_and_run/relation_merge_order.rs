//! `relation.merge(other)` orders by the receiver's orderings, then the
//! other relation's, as Rails' `Merger#merge_multi_values` does. The
//! runtime's merge carried the other relation's conditions and joins and
//! dropped its order.
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/task.rb", r##"class Task < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, actual, expected)
    raise "#{label}: #{actual.inspect}, Rails answers #{expected.inspect}" unless actual == expected
  end

  def self.run
    [["write", 2], ["plan", 1], ["test", 2], ["ship", 3]].each { |title, priority| Task.create!(title: title, priority: priority) }
    check("appended", Task.order(:priority).merge(Task.order(:title)).map(&:title), ["plan", "test", "write", "ship"])
    check("onto unordered", Task.where(priority: 2).merge(Task.order(title: :desc)).map(&:title), ["write", "test"])
    check("with conditions", Task.order(title: :desc).merge(Task.where(priority: [1, 3]).order(:priority)).map(&:title), ["ship", "plan"])
    check("reordered replaces", Task.order(:priority).merge(Task.reorder(title: :desc)).map(&:title), ["write", "test", "ship", "plan"])
    check("once", Task.order(:title).merge(Task.order(:title)).to_sql.scan("ORDER BY").size, 1)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "tasks", force: :cascade do |t|
    t.string "title"
    t.integer "priority"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn merge_appends_the_other_relations_order() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn merge_appends_the_other_relations_order_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

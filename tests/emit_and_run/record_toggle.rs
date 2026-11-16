//! `record.toggle(:attr)` sets the attribute to the negation of its query
//! method and answers the record without saving; `toggle!(:attr)` saves
//! it, as Rails' Persistence does. Ignored as a fork spec until the
//! compiler does; the expected values are ActiveRecord 8.1.4's answers.

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
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    t = Task.create!(title: "sweep", done: false)
    back = t.toggle(:done)
    check("toggle answers the record", back.equal?(t))
    check("toggle flips", t.done == true)
    check("toggle leaves the row", Task.find(t.id).done == false)
    # Rails: true, and the row reads false again
    check("toggle! answers true", t.toggle!(:done) == true)
    check("toggle! saves", Task.find(t.id).done == false)
    blank = Task.new(title: "dust", done: nil)
    blank.toggle(:done)
    check("nil toggles to true", blank.done == true)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "tasks", force: :cascade do |t|
    t.string "title"
    t.boolean "done"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: persistence"]
fn toggle_flips_an_attribute_by_its_query_method() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: persistence"]
fn toggle_flips_an_attribute_by_its_query_method_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

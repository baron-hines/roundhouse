//! `Model.new(nil)` builds a record and calls no `assign_attributes`,
//! like a bare `new` or `create!` (which hold on `main` already), while
//! `new({})` and `new(name: ...)` call it with what they were given, as
//! Rails' `initialize` assigns only the attributes it is handed.
//! Ignored as a fork spec until the compiler does; the expected values
//! are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/widget.rb", r##"class Widget < ApplicationRecord
  CALLS = []

  def assign_attributes(attrs)
    CALLS << attrs.keys.map(&:to_s).sort
    super
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    Widget.new
    Widget.create!
    # Rails: []
    check("a bare new or create assigns nothing", Widget::CALLS == [])
    Widget.new(nil)
    check("new(nil) assigns nothing", Widget::CALLS == [])
    Widget.new({})
    Widget.new(name: "cog")
    Widget.find(Widget.first.id)
    # Rails: [[], ["name"]]
    check("given attributes are assigned", Widget::CALLS == [[], ["name"]])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "widgets", force: :cascade do |t|
    t.string "name"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: persistence"]
fn new_nil_assigns_no_attributes() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: persistence"]
fn new_nil_assigns_no_attributes_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

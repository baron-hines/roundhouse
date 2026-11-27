//! In a hierarchy whose base sets its own `inheritance_column`, a query on
//! a subclass is scoped to it and loads as the subclass, `new` stamps the
//! subclass's name into that column, and a base query loads each row as
//! the class it names, as Rails does. Ignored as a fork spec until the
//! compiler does; the expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/instrument.rb", r##"class Instrument < ApplicationRecord
  self.inheritance_column = "family"
end
"##)
        .write("app/models/cello.rb", r##"class Cello < Instrument
end
"##)
        .write("app/models/flute.rb", r##"class Flute < Instrument
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    c = Cello.create!(name: "low")
    Flute.create!(name: "high")
    # Rails: "Cello"
    check("new stamps the column", c.family == "Cello")
    # Rails: ["low"], [Cello], 1
    cellos = Cello.where("name IS NOT NULL").to_a
    check("subclass query is scoped", cellos.map(&:name) == ["low"] && Cello.count == 1)
    check("subclass query loads the subclass", cellos.map(&:class) == [Cello])
    # Rails: [Cello, Flute]
    check("base query loads each class", Instrument.order(:id).map(&:class) == [Cello, Flute])
    # Rails: "Cello"
    check("first", Cello.first.class.name == "Cello")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "instruments", force: :cascade do |t|
    t.string "name"
    t.string "family"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: record queries"]
fn a_custom_inheritance_column_scopes_and_loads_subclasses() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: record queries"]
fn a_custom_inheritance_column_scopes_and_loads_subclasses_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

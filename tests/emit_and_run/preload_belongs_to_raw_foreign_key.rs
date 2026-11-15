//! A belongs_to preload reads the foreign-key column itself, past a
//! method the model defines under the column's name, as Rails'
//! `Preloader` reads `owner._read_attribute(foreign_key)`. The probe's
//! expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/river.rb", r##"class River < ApplicationRecord
end
"##)
        .write("app/models/bridge.rb", r##"class Bridge < ApplicationRecord
  belongs_to :river

  def river_id
    "river-#{self[:river_id]}"
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    a = River.create!(name: "avon")
    t = River.create!(name: "tay")
    Bridge.create!(span: 10, river: a)
    Bridge.create!(span: 20, river: t)
    bridges = Bridge.order(:span).preload(:river).to_a
    # Rails: ["avon", "tay"], and the override still answers
    check("preloaded", bridges.all? { |b| b.association(:river).loaded? })
    check("preload", bridges.map { |b| b.river.name } == ["avon", "tay"])
    check("override", bridges.first.river_id == "river-#{a.id}")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "rivers", force: :cascade do |t|
    t.string "name"
  end
  create_table "bridges", force: :cascade do |t|
    t.integer "span"
    t.integer "river_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_belongs_to_preload_reads_the_foreign_key_column() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_belongs_to_preload_reads_the_foreign_key_column_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

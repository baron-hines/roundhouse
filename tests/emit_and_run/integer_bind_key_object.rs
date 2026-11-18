//! An object bound to an integer column is cast through the column's
//! Integer type (`to_i`), not quoted as its `to_s`, in an equality, an
//! IN list and a negation, as Rails' QueryAttribute casts it. Ignored as
//! a fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/tag_key.rb", r##"class TagKey
  def initialize(value)
    @value = value
  end

  def to_i
    @value
  end

  def to_int
    @value
  end

  def to_s
    "key-#{@value}"
  end
end
"##)
        .write("app/models/parcel.rb", r##"class Parcel < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    Parcel.create!(depot_id: 2, name: "a")
    Parcel.create!(depot_id: 3, name: "b")
    # Rails: ["a"], ["a", "b"], ["b"]
    check("equality", Parcel.where(depot_id: TagKey.new(2)).pluck(:name) == ["a"])
    check("IN list", Parcel.where(depot_id: [TagKey.new(2), TagKey.new(3)]).order(:id).pluck(:name) == ["a", "b"])
    check("negation", Parcel.where.not(depot_id: TagKey.new(2)).pluck(:name) == ["b"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "parcels", force: :cascade do |t|
    t.integer "depot_id"
    t.string "name"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: relation queries"]
fn a_key_object_bound_to_an_integer_column_is_its_integer() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: relation queries"]
fn a_key_object_bound_to_an_integer_column_is_its_integer_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

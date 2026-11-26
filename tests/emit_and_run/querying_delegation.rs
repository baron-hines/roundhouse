//! `Model.reselect`, `Model.annotate` and the bang ordinals (`second!`
//! ... `second_to_last!`) delegate from the class to `all`, as Rails'
//! Querying does, and `relation.annotate_values` answers the comments as
//! given.
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/item.rb", r##"class Item < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, actual, expected)
    raise "#{label}: #{actual.inspect}, Rails answers #{expected.inspect}" unless actual == expected
  end

  def self.missing(label)
    yield
    raise "#{label}: found a record, Rails raises RecordNotFound"
  rescue ActiveRecord::RecordNotFound
    nil
  end

  def self.run
    %w[a b c d e f].each { |name| Item.create!(name: name) }
    check("bang ordinals", [Item.second!.name, Item.third!.name, Item.fourth!.name, Item.fifth!.name], ["b", "c", "d", "e"])
    check("second_to_last!", [Item.third_to_last!.name, Item.second_to_last!.name], ["d", "e"])
    missing("second!") { Item.where(name: "zzz").second! }
    check("reselect", Item.reselect(:name).order(:name).limit(2).map(&:name), ["a", "b"])
    check("relation reselect", Item.select(:id).reselect(:name).order(:name).map(&:name), ["a", "b", "c", "d", "e", "f"])
    check("annotate", Item.annotate("x").annotate("y", "z").annotate_values, ["x", "y", "z"])
    check("none", Item.all.annotate_values, [])
    check("comment", Item.annotate("probe").to_sql.include?("/* probe */"), true)
    check("annotated rows", Item.annotate("probe").where(name: "c").count, 1)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "items", force: :cascade do |t|
    t.string "name"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn reselect_annotate_and_bang_ordinals_reach_the_class() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn reselect_annotate_and_bang_ordinals_reach_the_class_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

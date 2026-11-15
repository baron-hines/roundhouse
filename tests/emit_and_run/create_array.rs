//! `create!(rows)` with an Array of attribute Hashes creates one record
//! per Hash and answers the records, each under the current scope's
//! attributes, as Rails' `create!` does
//! (`attributes.collect { |attr| create!(attr, &block) }`). Called from a
//! class method, the answer is an Array the caller maps; called through a
//! relation, each row takes the relation's `where` attributes.
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/tagging.rb", r##"class Tagging < ApplicationRecord
  def self.retag(post_id, tag_names)
    where(post_id: post_id).delete_all
    attrs = tag_names.uniq.map { |name| { post_id: post_id, tag_name: name } }
    taggings = create!(attrs)
    taggings.map(&:tag_name)
  end
end
"##)
        .write("app/models/tagging_factory.rb", r##"module TaggingFactory
  def self.create!(rows)
    [Tagging.new(tag_name: "built #{rows.size}")]
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, actual, expected)
    raise "#{label}: #{actual.inspect}, Rails answers #{expected.inspect}" unless actual == expected
  end

  def self.row(tagging)
    [tagging.post_id, tagging.tag_name, tagging.source]
  end

  def self.run
    Tagging.create!(post_id: 5, tag_name: "stale", source: "import")
    check("from a class method", Tagging.retag(5, ["ruby", "rails", "ruby"]), ["ruby", "rails"])
    check("rows", Tagging.order(:id).map { |t| row(t) }, [[5, "ruby", nil], [5, "rails", nil]])
    check("through a relation", Tagging.where(source: "editor").retag(8, ["sqlite"]), ["sqlite"])
    check("scoped row", row(Tagging.find_by(post_id: 8)), [8, "sqlite", "editor"])
    made = Tagging.create!([{ post_id: 9, tag_name: "a" }, { post_id: 9, tag_name: "b" }, { post_id: 9, tag_name: "c" }])
    check("class create!", [made.size, made.all?(&:persisted?)], [3, true])
    check("relation create", Tagging.where(source: "api").create([{ tag_name: "d" }]).map { |t| row(t) }, [[nil, "d", "api"]])
    check("an empty Array", Tagging.create!([]), [])
    tagged = Tagging.create!([{ post_id: 11, tag_name: "e" }, { post_id: 11, tag_name: "f" }]) { |t| t.source = "block" }
    check("a block runs per row", tagged.map { |t| row(t) }, [[11, "e", "block"], [11, "f", "block"]])
    check("an app factory's create!", TaggingFactory.create!([{ tag_name: "x" }, { tag_name: "y" }]).map(&:tag_name), ["built 2"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "taggings", force: :cascade do |t|
    t.integer "post_id"
    t.string "tag_name"
    t.string "source"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn create_of_an_array_creates_one_record_per_hash() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn create_of_an_array_creates_one_record_per_hash_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

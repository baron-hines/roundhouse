//! `owner.association(:name).build(attrs)` builds a record that joins the
//! has_many's target: the owner holds it, it carries the owner's key and
//! the owner as its inverse, and the owner's save saves it, as Rails'
//! `CollectionAssociation#build` does. Ignored as a fork spec until the
//! compiler does; the expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/orchard.rb", r##"class Orchard < ApplicationRecord
  has_many :trees
end
"##)
        .write("app/models/tree.rb", r##"class Tree < ApplicationRecord
  belongs_to :orchard
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    o = Orchard.create!(name: "hill")
    o.trees.create!(kind: "pear")
    t = o.association(:trees).build(kind: "plum")
    # Rails: held, keyed, inverse set, counted on top of the saved row
    check("owner holds it", o.trees.include?(t))
    check("owner key", t.orchard_id == o.id)
    check("inverse", t.orchard.equal?(o))
    check("size", o.trees.size == 2)
    o.save!
    check("owner save saves it", t.persisted? && Tree.where(orchard_id: o.id).count == 2)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "orchards", force: :cascade do |t|
    t.string "name"
  end
  create_table "trees", force: :cascade do |t|
    t.string "kind"
    t.integer "orchard_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: association targets"]
fn association_build_joins_the_has_many_target() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: association targets"]
fn association_build_joins_the_has_many_target_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

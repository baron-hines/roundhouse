//! A record built or created on a relation off a has_many
//! (`owner.items.where(...).build`) takes the relation's conditions, the
//! owner's key and the owner as its inverse, and joins the owner's target,
//! as Rails' `AssociationRelation` builds through its association.
//! Ignored as a fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/garden.rb", r##"class Garden < ApplicationRecord
  has_many :beds
end
"##)
        .write("app/models/bed.rb", r##"class Bed < ApplicationRecord
  belongs_to :garden
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    g = Garden.create!(name: "walled")
    b = g.beds.where(sunny: true).build(crop: "beans")
    # Rails: scoped, keyed, inverse set, held by the owner
    check("build conditions", b.sunny == true)
    check("build key", b.garden_id == g.id)
    check("build inverse", b.garden.equal?(g))
    check("build joins target", g.beds.include?(b))

    c = g.beds.where(sunny: false).create!(crop: "kale")
    # Rails: saved with the conditions, held by the owner
    check("create conditions", Bed.find(c.id).sunny == false && Bed.find(c.id).garden_id == g.id)
    check("create joins target", g.beds.target.include?(c))
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "gardens", force: :cascade do |t|
    t.string "name"
  end
  create_table "beds", force: :cascade do |t|
    t.string "crop"
    t.boolean "sunny"
    t.integer "garden_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: association targets"]
fn a_build_on_an_association_relation_joins_the_owners_target() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: association targets"]
fn a_build_on_an_association_relation_joins_the_owners_target_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

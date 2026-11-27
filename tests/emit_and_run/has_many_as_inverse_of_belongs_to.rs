//! A polymorphic has_many (`as:`) whose `inverse_of:` names a plain
//! belongs_to on the target seats the owner there when it builds a
//! record, so a `create!` runs the record's callbacks with the owner
//! already in place and saves its key, as Rails' `build_record` sets the
//! inverse instance. Ignored as a fork spec until the compiler does; the
//! expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/kitchen.rb", r##"class Kitchen < ApplicationRecord
  has_many :recipes, as: :origin, inverse_of: :kitchen
end
"##)
        .write("app/models/recipe.rb", r##"class Recipe < ApplicationRecord
  belongs_to :origin, polymorphic: true
  belongs_to :kitchen, optional: true

  before_validation do
    self.signature = kitchen ? kitchen.name : "none"
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    k = Kitchen.create!(name: "north")
    r = k.recipes.create!(title: "stew")
    # Rails: "north", the kitchen object itself, and its key saved
    check("callback sees the inverse", r.signature == "north")
    check("inverse is the owner", r.kitchen.equal?(k))
    check("inverse key saved", Recipe.find(r.id).kitchen_id == k.id)
    check("origin type", r.origin_type == "Kitchen")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "kitchens", force: :cascade do |t|
    t.string "name"
  end
  create_table "recipes", force: :cascade do |t|
    t.string "title"
    t.string "signature"
    t.string "origin_type"
    t.integer "origin_id"
    t.integer "kitchen_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: association targets"]
fn a_polymorphic_has_many_seats_its_owner_on_the_named_inverse() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: association targets"]
fn a_polymorphic_has_many_seats_its_owner_on_the_named_inverse_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

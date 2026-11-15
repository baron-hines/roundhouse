//! `owner.items.create!` holds the saved record in the has_many's target
//! without loading it, and `owner.items.update(attrs)` then updates the
//! records the owner holds in place, as Rails' `add_to_target` and the
//! proxy's `update` over its records do. The probe's expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/kennel.rb", r##"class Kennel < ApplicationRecord
  has_many :dogs
end
"##)
        .write("app/models/dog.rb", r##"class Dog < ApplicationRecord
  belongs_to :kennel
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    k = Kennel.create!(name: "ridge")
    d = k.dogs.create!(name: "rex", age: 2)
    # Rails: not loaded
    check("not loaded", !k.dogs.loaded?)
    updated = k.dogs.update(age: 3)
    # Rails: the held record itself is updated and saved
    check("held record updated", d.age == 3 && updated.length == 1 && updated.first.equal?(d))
    check("row updated", Dog.find(d.id).age == 3)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "kennels", force: :cascade do |t|
    t.string "name"
  end
  create_table "dogs", force: :cascade do |t|
    t.string "name"
    t.integer "age"
    t.integer "kennel_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_has_many_update_updates_the_records_its_create_holds() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_has_many_update_updates_the_records_its_create_holds_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

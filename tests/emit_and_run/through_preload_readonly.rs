//! A has_one :through whose scope is `readonly` preloads its hop on the
//! owners, then its source in a query of its own, as Rails' through
//! preloader does: each owner holds a read-only record, and the hop
//! records do not hold theirs, so a hop's own read loads a record that is
//! not read-only and not the owner's. The probe's expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/kennel.rb", r##"class Kennel < ApplicationRecord
end
"##)
        .write("app/models/dog.rb", r##"class Dog < ApplicationRecord
  belongs_to :kennel
end
"##)
        .write("app/models/collar.rb", r##"class Collar < ApplicationRecord
  belongs_to :dog
  has_one :home, -> { readonly }, through: :dog, source: :kennel
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.queries
    count = 0
    counter = ->(event) { count += 1 unless event.payload[:name] == "SCHEMA" }
    ActiveSupport::Notifications.subscribed(counter, "sql.active_record") do
      yield
    end
    count
  end

  def self.run
    oak = Kennel.create!(name: "oak")
    elm = Kennel.create!(name: "elm")
    rex = Dog.create!(name: "rex", kennel: oak)
    fido = Dog.create!(name: "fido", kennel: elm)
    Collar.create!(color: "red", dog: rex)
    Collar.create!(color: "blue", dog: fido)
    collars = nil
    # Rails: the collars, their dogs, the kennels
    check("one query each", queries { collars = Collar.order(:id).preload(:home).to_a } == 3)
    check("hop held", collars.all? { |c| c.association(:dog).loaded? && c.association(:home).loaded? })
    homes = nil
    check("read without a query", queries { homes = collars.map(&:home) } == 0)
    check("targets", homes.map(&:name) == ["oak", "elm"])
    check("read-only", homes.all?(&:readonly?))
    # Rails: the hop's own kennel is not loaded, and is another record
    check("hop records without the source", collars.none? { |c| c.dog.association(:kennel).loaded? })
    own = nil
    check("hop source read", queries { own = collars.first.dog.kennel } == 1)
    check("hop source writable", own.name == "oak" && !own.readonly? && !own.equal?(homes.first))
    collars = Collar.order(:id).preload(dog: :kennel).to_a
    held = collars.first.dog.kennel
    # Rails: a dog already holding its kennel answers it, with no query, and
    # that record stays writable
    check("held sources", queries { ActiveRecord::Associations::Preloader.new(records: collars, associations: :home).call } == 0)
    check("held source answers", collars.first.home.equal?(held) && !collars.first.home.readonly?)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "kennels", force: :cascade do |t|
    t.string "name"
  end
  create_table "dogs", force: :cascade do |t|
    t.string "name"
    t.integer "kennel_id"
  end
  create_table "collars", force: :cascade do |t|
    t.string "color"
    t.integer "dog_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_readonly_through_preloads_its_source_apart_from_the_hop() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_readonly_through_preloads_its_source_apart_from_the_hop_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

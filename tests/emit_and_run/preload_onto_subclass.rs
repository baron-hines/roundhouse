//! A scoped has_many and a polymorphic has_one whose class is an STI
//! subclass preload the subclass's rows only, each as the subclass, in
//! one query per association, as Rails' `Preloader` does. Ignored as a
//! fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/zoo.rb", r##"class Zoo < ApplicationRecord
  has_many :big_cats, -> { where("weight > ?", 100) }, class_name: "Cat"
  has_one :mascot, as: :home, class_name: "Cat"
end
"##)
        .write("app/models/animal.rb", r##"class Animal < ApplicationRecord
  belongs_to :zoo, optional: true
  belongs_to :home, polymorphic: true, optional: true
end
"##)
        .write("app/models/cat.rb", r##"class Cat < Animal
end
"##)
        .write("app/models/bird.rb", r##"class Bird < Animal
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    a = Zoo.create!(name: "north")
    b = Zoo.create!(name: "south")
    Cat.create!(name: "tiger", weight: 200, zoo: a)
    Cat.create!(name: "lynx", weight: 20, zoo: a)
    Bird.create!(name: "condor", weight: 150, zoo: a)
    Cat.create!(name: "lion", weight: 180, zoo: b, home: b)
    Bird.create!(name: "owl", weight: 2, home: a)
    queries = 0
    counter = ->(event) { queries += 1 unless event.payload[:name] == "SCHEMA" }
    zoos = nil
    ActiveSupport::Notifications.subscribed(counter, "sql.active_record") do
      zoos = Zoo.order(:id).preload(:big_cats, :mascot).to_a
    end
    # Rails: three queries (zoos, big cats, mascots)
    check("one query per association", queries == 3)
    check("scoped has_many", zoos.map { |z| z.big_cats.map(&:name) } == [["tiger"], ["lion"]])
    check("as the subclass", zoos.flat_map(&:big_cats).all? { |c| c.class == Cat })
    check("polymorphic has_one", zoos.map { |z| z.mascot && z.mascot.name } == [nil, "lion"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "zoos", force: :cascade do |t|
    t.string "name"
  end
  create_table "animals", force: :cascade do |t|
    t.string "type"
    t.string "name"
    t.integer "weight"
    t.integer "zoo_id"
    t.string "home_type"
    t.integer "home_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: preload"]
fn a_scoped_or_polymorphic_preload_onto_a_subclass_is_one_query() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: preload"]
fn a_scoped_or_polymorphic_preload_onto_a_subclass_is_one_query_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

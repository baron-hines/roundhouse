//! A has_many preload seats the owner on each row it loads before the
//! row's `after_initialize` runs, so the callback reads the owner object
//! itself, scoped association or not, as Rails' `Preloader` sets the
//! inverse in the block it hands to the load. Ignored as a fork spec until
//! the compiler does; the expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/museum.rb", r##"class Museum < ApplicationRecord
  has_many :paintings, inverse_of: :museum
  has_many :old_paintings, -> { where("year < ?", 1800) }, class_name: "Painting", inverse_of: :museum
end
"##)
        .write("app/models/painting.rb", r##"class Painting < ApplicationRecord
  belongs_to :museum
  attr_reader :museum_at_init

  after_initialize do
    @museum_at_init = association(:museum).loaded? ? museum : nil
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    m = Museum.create!(name: "harbor")
    Painting.create!(title: "dawn", year: 1750, museum: m)
    Painting.create!(title: "dusk", year: 1900, museum: m)

    museums = Museum.preload(:paintings).to_a
    owner = museums.first
    # Rails: every row saw its owner during after_initialize
    check("unscoped", owner.paintings.length == 2 && owner.paintings.all? { |p| p.museum_at_init.equal?(owner) })

    museums = Museum.preload(:old_paintings).to_a
    owner = museums.first
    check("scoped", owner.old_paintings.map(&:title) == ["dawn"] && owner.old_paintings.first.museum_at_init.equal?(owner))
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "museums", force: :cascade do |t|
    t.string "name"
  end
  create_table "paintings", force: :cascade do |t|
    t.string "title"
    t.integer "year"
    t.integer "museum_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: preload"]
fn a_preloaded_row_holds_its_owner_before_after_initialize() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: preload"]
fn a_preloaded_row_holds_its_owner_before_after_initialize_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

//! Preloading a belongs_to whose scope reads its owner (`->(record) {
//! ... }`) gives each owner the target its own scope admits, the owners
//! that share a scope loading together, as Rails' `Preloader` groups an
//! instance-dependent scope by owner. The plain reader doesn't apply such
//! a scope here either (analysis warns), and the probe checks it after
//! the preload. Ignored as a fork spec until the compiler does both; the
//! expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/league.rb", r##"class League < ApplicationRecord
end
"##)
        .write("app/models/fixture.rb", r##"class Fixture < ApplicationRecord
  belongs_to :league, ->(fixture) { where(season: fixture.season) }, optional: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    spring = League.create!(name: "spring cup", season: "spring")
    autumn = League.create!(name: "autumn cup", season: "autumn")
    Fixture.create!(venue: "a", season: "spring", league_id: spring.id)
    Fixture.create!(venue: "b", season: "autumn", league_id: spring.id)
    Fixture.create!(venue: "c", season: "autumn", league_id: autumn.id)
    Fixture.create!(venue: "d", season: "spring", league_id: spring.id)
    preloaded = Fixture.order(:venue).preload(:league).map { |f| f.league && f.league.name }
    read = Fixture.order(:venue).map { |f| f.league && f.league.name }
    # Rails: ["spring cup", nil, "autumn cup", "spring cup"] both ways
    check("preload", preloaded == ["spring cup", nil, "autumn cup", "spring cup"])
    check("reader", read == ["spring cup", nil, "autumn cup", "spring cup"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "leagues", force: :cascade do |t|
    t.string "name"
    t.string "season"
  end
  create_table "fixtures", force: :cascade do |t|
    t.string "venue"
    t.string "season"
    t.integer "league_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: preload"]
fn a_preloaded_instance_dependent_belongs_to_follows_each_owners_scope() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: preload"]
fn a_preloaded_instance_dependent_belongs_to_follows_each_owners_scope_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

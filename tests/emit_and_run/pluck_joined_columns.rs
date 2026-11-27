//! A pluck of `"table.column"` strings of a joined table beside an
//! `Arel.sql` expression names that table as written and casts each
//! column through the joined model (a boolean, a date), as Rails' pluck
//! looks the type up through the join. Ignored as a fork spec until the
//! compiler does; the expected values are ActiveRecord 8.1.4's answers.

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
        .write("app/models/club.rb", r##"class Club < ApplicationRecord
  belongs_to :league
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    l = League.create!(name: "upper", open: true, opened_on: Date.new(2024, 2, 3))
    Club.create!(name: "rovers", league: l, points: 7)
    rows = Club.joins(:league).pluck("leagues.open", "leagues.opened_on", Arel.sql("clubs.points * 2"))
    # Rails: [[true, Sat, 03 Feb 2024, 14]]
    check("joined values", rows == [[true, Date.new(2024, 2, 3), 14]])
    check("boolean cast", rows.first[0] == true)
    check("date cast", rows.first[1].is_a?(Date))
    # Rails: ["upper"]
    check("qualified name", Club.joins(:league).pluck("leagues.name") == ["upper"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "leagues", force: :cascade do |t|
    t.string "name"
    t.boolean "open"
    t.date "opened_on"
  end
  create_table "clubs", force: :cascade do |t|
    t.string "name"
    t.integer "league_id"
    t.integer "points"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: record queries"]
fn a_pluck_of_joined_columns_casts_through_the_joined_model() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: record queries"]
fn a_pluck_of_joined_columns_casts_through_the_joined_model_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

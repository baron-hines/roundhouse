//! `includes(:assoc).where(assoc: {...})` eager-loads the association
//! under its own name as the JOIN's alias, and for an association named
//! after an SQL keyword that alias is quoted wherever the statement reads
//! it, as Rails' predicate builder and JoinDependency write it. Ignored
//! as a fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/team.rb", r##"class Team < ApplicationRecord
  has_one :group, class_name: "Squad"
end
"##)
        .write("app/models/squad.rb", r##"class Squad < ApplicationRecord
  belongs_to :team, optional: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    reds = Team.create!(name: "reds")
    blues = Team.create!(name: "blues")
    Squad.create!(team: reds, sport: "chess")
    Squad.create!(team: blues, sport: "polo")
    rows = Team.includes(:group).where(group: { sport: "chess" }).to_a
    # Rails: [["reds", "chess"]]
    check("the eager JOIN reads its quoted alias", rows.map { |t| [t.name, t.group.sport] } == [["reds", "chess"]])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "teams", force: :cascade do |t|
    t.string "name"
  end
  create_table "squads", force: :cascade do |t|
    t.integer "team_id"
    t.string "sport"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: relation queries"]
fn a_where_on_an_included_association_reads_its_quoted_alias() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: relation queries"]
fn a_where_on_an_included_association_reads_its_quoted_alias_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

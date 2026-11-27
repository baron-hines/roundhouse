//! A query that joins a belongs_to off its own table and the same
//! belongs_to off a self-join keeps each hop on its own side: the first
//! joins off the unaliased FROM table, the second off the self-join's
//! alias, as Rails' join dependency writes them. Ignored as a fork spec
//! until the compiler does; the expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/guild.rb", r##"class Guild < ApplicationRecord
end
"##)
        .write("app/models/smith.rb", r##"class Smith < ApplicationRecord
  belongs_to :guild
  belongs_to :master, class_name: "Smith", optional: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    iron = Guild.create!(name: "iron")
    gold = Guild.create!(name: "gold")
    elder = Smith.create!(name: "elder", guild: gold)
    Smith.create!(name: "pupil", guild: iron, master: elder)
    Smith.create!(name: "peer", guild: gold, master: elder)
    both = Smith.joins(:guild, master: :guild)
    # Rails: ["peer", "pupil"]
    check("both hops join", both.order(:name).pluck(:name) == %w[peer pupil])
    # Rails: ["pupil"]
    check("own hop off the FROM table", both.where(guilds: { name: "iron" }).pluck(:name) == ["pupil"])
    # Rails: [["peer", "gold", "gold"], ["pupil", "iron", "gold"]]
    rows = both.order(:name).pluck(:name, "guilds.name", "guilds_smiths.name")
    check("each side reads its own guild", rows == [["peer", "gold", "gold"], ["pupil", "iron", "gold"]])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "guilds", force: :cascade do |t|
    t.string "name"
  end
  create_table "smiths", force: :cascade do |t|
    t.string "name"
    t.integer "guild_id"
    t.integer "master_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: record queries"]
fn a_hop_off_the_query_table_and_one_off_a_self_join_stay_apart() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: record queries"]
fn a_hop_off_the_query_table_and_one_off_a_self_join_stay_apart_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

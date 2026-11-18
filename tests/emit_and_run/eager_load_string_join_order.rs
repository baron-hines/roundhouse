//! An eager load writes its LEFT OUTER JOINs ahead of the SQL-string
//! joins, whether the string join was written before or after the
//! `includes`, and a string join written with surrounding whitespace is
//! stripped and still goes last, as Rails' `build_joins` does. Ignored as
//! a fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/genre.rb", r##"class Genre < ApplicationRecord
end
"##)
        .write("app/models/song.rb", r##"class Song < ApplicationRecord
  belongs_to :genre, optional: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.load(rel)
    statements = []
    counter = ->(event) { statements << event.payload[:sql] unless event.payload[:name] == "SCHEMA" }
    rows = nil
    ActiveSupport::Notifications.subscribed(counter, "sql.active_record") { rows = rel.to_a }
    [statements.last, rows]
  end

  def self.run
    folk = Genre.create!(kind: "folk")
    noise = Genre.create!(kind: "noise")
    Song.create!(name: "a", genre: folk)
    Song.create!(name: "b", genre: noise)
    join = "LEFT JOIN genres AS other ON other.id = songs.genre_id"
    padded = "\n  LEFT JOIN genres AS other ON other.id = songs.genre_id\n  "
    before = Song.joins(join).eager_load(:genre).where.not(genres: { kind: "noise" })
    after = Song.eager_load(:genre).joins(join).where.not(genres: { kind: "noise" })
    stripped = Song.joins(padded).eager_load(:genre).where.not(genres: { kind: "noise" })
    sql = nil
    [before, after, stripped].each do |rel|
      sql, rows = load(rel)
      # Rails: FROM "songs" LEFT OUTER JOIN "genres" ... LEFT JOIN genres AS other ...
      check("the eager JOIN first", sql.index("LEFT OUTER JOIN") < sql.index("LEFT JOIN genres AS other"))
      check("the rows", rows.map { |l| [l.name, l.genre.kind] } == [["a", "folk"]])
    end
    check("the string join stripped", sql.include?("songs.genre_id WHERE"))
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "genres", force: :cascade do |t|
    t.string "kind"
  end
  create_table "songs", force: :cascade do |t|
    t.string "name"
    t.integer "genre_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: relation queries"]
fn an_eager_load_writes_its_joins_before_string_joins() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: relation queries"]
fn an_eager_load_writes_its_joins_before_string_joins_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

//! `relation.order_values` answers Rails' orderings: an
//! `Arel::Nodes::Ascending` or `Descending` of the column's attribute for
//! a Symbol or Hash order, a node as given, and a String as written, as
//! Rails' QueryMethods hold them. A list read back from `order_values`
//! orders a relation again the same way.
//! The probe's expected values are ActiveRecord 8.1.4's answers. CRuby
//! only: the ordering nodes live in the CRuby overlay's Arel, and Spinel
//! reports `Arel::Nodes::Ordering` as unsupported.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/song.rb", r##"class Song < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, actual, expected)
    raise "#{label}: #{actual.inspect}, Rails answers #{expected.inspect}" unless actual == expected
  end

  def self.shape(values)
    values.map do |value|
      if value.is_a?(Arel::Nodes::Ordering)
        "#{value.class.name.split("::").last} #{value.expr.name}"
      elsif value.is_a?(String)
        "String #{value}"
      else
        value.class.name
      end
    end
  end

  def self.run
    Song.create!(plays: 10, title: "Blue")
    Song.create!(plays: 30, title: "Amber")
    Song.create!(plays: 10, title: "Cyan")
    charts = Song.order(plays: :desc, title: :asc).limit(5)
    check("hash", shape(charts.order_values), ["Descending plays", "Ascending title"])
    leading = charts.order_values.first
    check("node class", [leading.is_a?(Arel::Nodes::Descending), leading.is_a?(Arel::Nodes::Ascending)], [true, false])
    check("symbol", shape(Song.order(:title).order_values), ["Ascending title"])
    check("string", shape(Song.order("songs.title DESC").order_values), ["String songs.title DESC"])
    check("node", shape(Song.order(Song.arel_table[:plays].asc).order_values), ["Ascending plays"])
    check("reorder", shape(Song.order(:id).reorder(title: :desc).order_values), ["Descending title"])
    check("under scoping", shape(Song.order(plays: :desc).scoping { Song.all.order_values }), ["Descending plays"])
    check("rows", charts.pluck(:title), ["Amber", "Blue", "Cyan"])
    check("reordered by the list", Song.order(:id).reorder(charts.order_values).pluck(:title), ["Amber", "Blue", "Cyan"])
    lowered = Arel::Nodes::Ascending.new(Arel.sql("LOWER(songs.title)"))
    check("a literal term", [lowered.expr.is_a?(Arel::Nodes::SqlLiteral), lowered.expr.to_s], [true, "LOWER(songs.title)"])
    check("an attribute term", Arel::Nodes::Descending.new(Song.arel_table[:plays]).expr.name, "plays")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "songs", force: :cascade do |t|
    t.integer "plays"
    t.string "title"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn order_values_answers_rails_orderings() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

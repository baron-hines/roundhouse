//! maybe's `Family::DataExporter`: `{ type: "Category", data: category.as_json }.to_json`.
//! The line is the record's `serializable_hash` under `data`: every column,
//! a timestamp as `TimeWithZone#as_json` writes it, `xmlschema(3)`, which
//! ends in `Z` in a UTC app (Spinel's `json_time` wrote `+00:00`).

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit(
        "app/models/article.rb",
        "class Article < ApplicationRecord\n",
        "class Article < ApplicationRecord\n  def export_line = { type: \"Article\", data: as_json }.to_json\n\n  def self.export_lines = all.map { |a| { type: \"Article\", data: a.as_json, tags: [a.as_json] }.to_json }\n",
    )
}

const ASSERTIONS: &str = r##"
require "json"
a = Article.create!(title: "Hello", body: "A body long enough for validation.")
[a.export_line, Article.export_lines.first].each do |line|
  got = JSON.parse(line)
  raise line unless got["type"] == "Article"
  data = got["data"]
  raise line unless data["id"] == a.id && data["title"] == "Hello" && data["body"] == "A body long enough for validation."
  raise line unless data.keys.sort == %w[body created_at id title updated_at]
  raise line unless data["created_at"] =~ /\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z\z/
end
raise "tags" unless JSON.parse(Article.export_lines.first)["tags"].first["title"] == "Hello"
puts "as_json in to_json passed"
"##;

#[test]
fn a_records_as_json_inside_a_literal_to_json() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("as_json in to_json passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_records_as_json_inside_a_literal_to_json_on_spinel() {
    let run = app().run_spinel_with_db(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("as_json in to_json passed"), "{}", run.stdout);
}


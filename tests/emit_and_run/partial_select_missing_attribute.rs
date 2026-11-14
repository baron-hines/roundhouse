//! A column a partial `select` left out raises
//! `ActiveModel::MissingAttributeError` when it is read, as Rails'
//! generated attribute reader raises, also through a module the model
//! includes whose reader answers `super`. A column the model's own method
//! answers without reading it still answers.
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/publishable.rb", r##"module Publishable
  def status
    super
  end
end
"##)
        .write("app/models/article.rb", r##"class Article < ApplicationRecord
  include Publishable

  def byline
    "staff"
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, actual, expected)
    raise "#{label}: #{actual.inspect}, Rails answers #{expected.inspect}" unless actual == expected
  end

  def self.read
    yield.inspect
  rescue ActiveModel::MissingAttributeError
    "missing"
  end

  def self.run
    article = Article.create!(author_id: 3, title: "Hello", status: "draft", byline: "b")
    partial = Article.where(id: article.id).select(:author_id, :title).take!
    check("selected", read { partial.title }, "\"Hello\"")
    check("left out", read { partial.status }, "missing")
    check("own method", read { partial.byline }, "\"staff\"")
    check("has_attribute?", partial.has_attribute?(:status), false)
    check("whole row", read { Article.find(article.id).status }, "\"draft\"")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "articles", force: :cascade do |t|
    t.integer "author_id"
    t.string "title"
    t.string "status"
    t.string "byline"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_column_a_partial_select_left_out_raises_when_read() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_column_a_partial_select_left_out_raises_when_read_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

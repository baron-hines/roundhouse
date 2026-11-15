//! A column a partial `select` left out raises
//! `ActiveModel::MissingAttributeError` when it is read, as Rails'
//! generated attribute reader raises, also through a module the model
//! includes whose reader answers `super`. A column the model's own method
//! answers without reading it still answers. Serializing the record
//! (`as_json` / `serializable_hash` / `to_json`) writes only its
//! `attribute_names`, as Rails' `serializable_hash` does: the loaded
//! columns and the primary key, never a left-out column.
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
    whole = Article.find(article.id)
    check("partial as_json", partial.as_json.keys.sort, ["author_id", "id", "title"])
    check("partial serializable_hash", partial.serializable_hash.slice("author_id", "title"), { "author_id" => 3, "title" => "Hello" })
    check("partial serializable_hash keys", partial.serializable_hash.keys.sort, ["author_id", "id", "title"])
    check("partial to_json", [partial.to_json.include?("\"title\":\"Hello\""), partial.to_json.include?("status")], [true, false])
    check("partial to_json only", partial.to_json(only: [:title, :status]), "{\"title\":\"Hello\"}")
    check("whole to_json", whole.to_json.include?("\"status\":\"draft\""), true)
    check("whole as_json", whole.as_json.keys.sort, ["author_id", "byline", "id", "status", "title"])
    check("whole serializable_hash", whole.serializable_hash, { "id" => article.id, "author_id" => 3, "title" => "Hello", "status" => "draft", "byline" => "staff" })
    assigned = Article.where(id: article.id).select(:title).take!
    assigned.status = "published"
    check("assigned reads", read { assigned.status }, "\"published\"")
    check("assigned has_attribute?", assigned.has_attribute?(:status), true)
    check("assigned as_json", assigned.as_json.keys.sort, ["id", "status", "title"])
    assigned[:author_id] = 4
    check("index-assigned reads", read { assigned.author_id }, "4")
    check("index-assigned as_json", assigned.as_json["author_id"], 4)
    check("dynamic read", read { index(partial, :status) }, "missing")
    check("dynamic read loaded", read { index(partial, :title) }, "\"Hello\"")
    check("read_attribute", read { partial.read_attribute(:status) }, "nil")
    check("read_attribute String", read { named(partial, "status") }, "nil")
    check("read_attribute String loaded", read { named(partial, "title") }, "\"Hello\"")
    cleared = Article.where(id: article.id).select(:id, :title).take!
    cleared.status = nil
    cleared.save!
    check("nil assignment saved", article.reload.status, nil)
    check("assigned nothing pending", cleared.changed?, false)
    Article.where(id: article.id).update_all(status: "later")
    cleared.save!
    check("later save keeps another write", article.reload.status, "later")
    rolled = Article.where(id: article.id).select(:id, :title).take!
    rolled.status = nil
    Article.transaction do
      rolled.save!
      raise ActiveRecord::Rollback
    end
    check("nil assignment rolled back", article.reload.status, "later")
    rolled.save!
    check("nil assignment saved after a rollback", article.reload.status, nil)
    Article.where(id: article.id).update_all(status: "kept")
    direct = Article.where(id: article.id).select(:id, :title).take!
    direct.status = nil
    Article.transaction do
      direct.update_column(:status, nil)
      raise ActiveRecord::Rollback
    end
    check("update_column rolled back", article.reload.status, "kept")
    direct.save!
    check("update_column cleared the change before its rollback", article.reload.status, "kept")
    later = Article.where(id: article.id).select(:id, :title).take!
    Article.transaction do
      later.title = "Retitled"
      later.save!
      later.status = nil
      later.save!
      raise ActiveRecord::Rollback
    end
    check("second save rolled back", [article.reload.title, article.reload.status], ["Hello", "kept"])
    later.save!
    check("retry after a two-save rollback", [article.reload.title, article.reload.status], ["Retitled", "kept"])
  end

  def self.index(record, key)
    record[key]
  end

  def self.named(record, key)
    record.read_attribute(key)
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

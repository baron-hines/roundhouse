# The oracle for tests/sti_untyped.rs (#1415): a subclass of a concrete
# model whose table has no `type` column, on activerecord 8.1.4.
#
#   ruby tests/sti_untyped/oracle.rb
#
# Needs activerecord 8.1.4 and sqlite3. Without the column it isn't
# STI: the subclass shares the table, its finders filter nothing, and
# every row they load is the subclass's class. `none` loads nothing,
# and an aggregate (`count`, `pluck`, `exists?`) answers a value, not
# records. It prints:
#
#   {table: "articles", base: "Article"}
#   {count: [2, 2]}
#   {first: "Feature", find: "Feature", all: ["Feature", "Feature"], where: ["Feature"], find_by: "Feature", scope: ["Feature"]}
#   {new: "Feature", create: "Feature"}
#   {none: [], count: 1, pluck: ["f"], exists: true}
#   SQL: SELECT "articles".* FROM "articles" WHERE "articles"."title" = 'f'
gem "activerecord", "8.1.4"
require "active_record"
ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
ActiveRecord::Schema.verbose = false
ActiveRecord::Schema.define do
  create_table(:articles) { |t| t.string :title; t.text :body; t.timestamps }
end
class ApplicationRecord < ActiveRecord::Base
  primary_abstract_class
end
class Article < ApplicationRecord
  scope :titled_f, -> { where(title: "f") }
end
class Feature < Article
end
p table: Feature.table_name, base: Feature.base_class.name
article = Article.create!(title: "a")
Feature.create!(title: "f")
p count: [Article.count, Feature.count]
names = ->(records) { records.map { |r| r.class.name } }
p first: Feature.order(:id).first.class.name, find: Feature.find(article.id).class.name,
  all: names.(Feature.order(:id)), where: names.(Feature.where(title: "f")),
  find_by: Feature.find_by(title: "f").class.name, scope: names.(Feature.titled_f)
p new: Feature.new.class.name, create: Feature.create!(title: "n").class.name
p none: Feature.none.to_a, count: Feature.where(title: "f").count,
  pluck: Feature.where(title: "f").pluck(:title), exists: Feature.where(title: "f").exists?
puts "SQL: #{Feature.where(title: "f").to_sql}"

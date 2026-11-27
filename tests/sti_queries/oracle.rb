# The oracle for tests/sti_queries.rs (#1415): an STI subclass's
# queries, with a `type` column, on activerecord 8.1.4.
#
#   ruby tests/sti_queries/oracle.rb
#
# Needs activerecord 8.1.4 and sqlite3. It prints "sti queries passed"
# and the SQL Rails filters a subclass on:
#
#   SELECT "articles".* FROM "articles" WHERE "articles"."type" IN
#   ('Feature', 'SpecialFeature') AND "articles"."title" = 'f'
#
# The table and Article are real-blog's, as the test edits them: a
# `type` column and one named scope (its association and broadcast left out). It loads
# the test's own files: Feature < Article, SpecialFeature < Feature,
# the StiProbe app class and the script that checks each row.
gem "activerecord", "8.1.4"
require "active_record"
ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
ActiveRecord::Schema.verbose = false
ActiveRecord::Schema.define do
  create_table(:articles) { |t| t.string :title; t.string :type; t.text :body; t.timestamps }
end
class ApplicationRecord < ActiveRecord::Base
  primary_abstract_class
end
class Article < ApplicationRecord
  scope :titled_a_or_f, -> { where(title: ["a", "f"]) }
  validates :title, presence: true
  validates :body, presence: true, length: { minimum: 10 }
end
# HOOKED=1 adds the hook tests/sti_queries.rs's hooked case writes, and
# its checks.
if ENV["HOOKED"]
  class Article
    after_initialize :mark_hooked

    def mark_hooked
      @hook_calls = (@hook_calls || 0) + 1
      @hook_title = title
    end

    def hook_seen
      [@hook_calls, @hook_title]
    end
  end
end
%w[feature special_feature probe script].each { |f| load File.join(__dir__, "#{f}.rb") }
%w[hook_probe hooked].each { |f| load File.join(__dir__, "#{f}.rb") } if ENV["HOOKED"]
puts "SQL: #{Feature.where(title: "f").to_sql}"

# The oracle for tests/sti_disabled.rs: `self.inheritance_column = nil`
# (or `false`) on a model with a `type` column, on activerecord 8.1.4.
#
#   ruby tests/sti_disabled/oracle.rb
#
# Needs activerecord 8.1.4 and sqlite3. Either value turns STI off for
# the model (`false` is stored as the column name "false", which no table
# has). `type` is a plain attribute: a row says what it was given, and
# loads as the model. A subclass shares the table, its finders filter
# nothing, its rows load as the subclass, and its constructor leaves
# `type` alone. It prints:
#
#   {note: "", card: "false"}
#   {type: "Whatever", class: "Note"}
#   {card: ["Card", "Mention"]}
#   {count: [2, 2], first: "Sticky", created_type: nil}
#   SQL: SELECT "notes".* FROM "notes" WHERE "notes"."title" = 'a'
#   {nil_then_type: {ic: "type", counts: [1, 1], stamped: "Pin"}}
#   {type_then_nil: {ic: "", counts: [1, 1], stamped: nil}}
#   {subclass_off: {ic: "", counts: [2, 2], stamped: nil, first: "Opted", base_rows: ["Post", "Post"]}}
#   {sibling_on: {counts: [3, 3, 1], row: ["Tacked", "Tacked"]}}
#   {abstract_off: {ic: "", counts: [2, 2], stamped: nil, first: "Wide2"}}
#
# The last assignment wins (`nil` then `"type"` is STI on), and a
# subclass that sets `nil` itself isn't STI while its base still is: a
# sibling without the setting is scoped and stamps its type.
# An abstract parent's setting is inherited.
gem "activerecord", "8.1.4"
require "active_record"
ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
ActiveRecord::Schema.verbose = false
ActiveRecord::Schema.define do
  create_table(:notes) { |t| t.string :title; t.string :type }
  create_table(:cards) { |t| t.string :title; t.string :type }
  %i[boards decks posts].each { |table| create_table(table) { |t| t.string :title; t.string :type } }
end
class ApplicationRecord < ActiveRecord::Base
  primary_abstract_class
end
class Note < ApplicationRecord
  self.inheritance_column = nil
end
class Card < ApplicationRecord
  self.inheritance_column = false
end
class Sticky < Note
end
p note: Note.inheritance_column, card: Card.inheritance_column
note = Note.create!(title: "a", type: "Whatever")
p type: Note.find(note.id).type, class: Note.find(note.id).class.name
Card.create!(title: "c", type: "Mention")
p card: [Card.first.class.name, Card.first.type]
sticky = Sticky.create!(title: "s")
p count: [Note.count, Sticky.count], first: Sticky.order(:id).first.class.name, created_type: sticky.type
puts "SQL: #{Sticky.where(title: "a").to_sql}"
class Board < ApplicationRecord
  self.inheritance_column = nil
  self.inheritance_column = "type"
end
class Pin < Board
end
class Deck < ApplicationRecord
  self.inheritance_column = "type"
  self.inheritance_column = nil
end
class Wide < Deck
end
class Post < ApplicationRecord
end
class Opted < Post
  self.inheritance_column = nil
end
pin = Pin.create!(title: "p")
p nil_then_type: { ic: Board.inheritance_column, counts: [Board.count, Pin.count], stamped: pin.type }
wide = Wide.create!(title: "w")
p type_then_nil: { ic: Deck.inheritance_column, counts: [Deck.count, Wide.count], stamped: wide.type }
Post.create!(title: "p")
opted = Opted.create!(title: "o")
p subclass_off: { ic: Opted.inheritance_column, counts: [Post.count, Opted.count], stamped: opted.type,
  first: Opted.order(:id).first.class.name, base_rows: Post.order(:id).map { |r| r.class.name } }
class Tacked < Post
end
tacked = Tacked.create!(title: "t")
p sibling_on: { counts: [Post.count, Opted.count, Tacked.count], row: [Post.find(tacked.id).class.name, Post.find(tacked.id).type] }
class LegacyRecord < ActiveRecord::Base
  self.abstract_class = true
  self.inheritance_column = nil
end
class Legacy < LegacyRecord
  self.table_name = "legacies"
end
class Wide2 < Legacy
end
ActiveRecord::Schema.define { create_table(:legacies) { |t| t.string :title; t.string :type } }
wide2 = Wide2.create!(title: "w")
Legacy.create!(title: "l")
p abstract_off: { ic: Legacy.inheritance_column, counts: [Legacy.count, Wide2.count], stamped: wide2.type,
  first: Wide2.order(:id).first.class.name }

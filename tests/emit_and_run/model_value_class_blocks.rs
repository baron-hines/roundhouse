//! A value class a model declares in its body with `Data.define(...) do
//! … end` or `Struct.new(...) do … end` keeps the methods its block
//! defines, endless or not, and the model's own methods build and read
//! it. A block method reads the model's constant and a sibling value
//! class, as the block's lexical scope is the model. The probe's
//! expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/recipe.rb", r##"class Recipe < ApplicationRecord
  SHORTLIST = 2

  Tally = Data.define(:recipe, :words) do
    def distinct = words.uniq.sort
    def count = words.size
    def shortlist = distinct.first(SHORTLIST)
    def timing = Window.new(recipe.prep_minutes, recipe.cook_minutes)

    def summary
      "#{recipe.title}: #{count} words, #{distinct.size} distinct"
    end
  end

  Window = Struct.new(:low, :high) do
    def span = high - low

    def cover?(value)
      value >= low && value <= high
    end
  end

  def self.tallies(recipes)
    recipes.map { |r| Tally.new(r, r.steps.split(" ")) }
  end

  def minutes_range
    Window.new(prep_minutes, prep_minutes + cook_minutes)
  end
end
"##)
        .write("app/models/admin/menu.rb", r##"module Admin
  class Menu < ApplicationRecord
    Pair = Data.define(:first, :second) do
      def total = first + second
    end

    Scale = Struct.new(:factor) do
      def apply(value) = value * factor
    end

    def self.priced(first, second)
      Scale.new(3).apply(Pair.new(first, second).total)
    end
  end
end
"##)
        .write("app/models/memo.rb", r##"class Memo < ApplicationRecord
  JSON = Struct.new(:tag) do
    def label = tag.upcase
  end
end
"##)
        .write("app/models/ticket.rb", r##"class Ticket < ApplicationRecord
  class JSON
    def self.shout(text) = text + "!"
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  # Rails: [["boil", "salt", "water"], 4, "soup: 4 words, 3 distinct"]
  def self.data_block_methods
    soup = Recipe.create!(title: "soup", steps: "boil water salt water", prep_minutes: 5, cook_minutes: 20)
    tally = Recipe.tallies([soup]).first
    [tally.distinct, tally.count, tally.summary]
  end

  # The block reads the model's constant and a sibling value class: Rails: [["boil", "salt"], 15]
  def self.data_block_reads_the_model_scope
    soup = Recipe.create!(title: "soup", steps: "boil water salt water", prep_minutes: 5, cook_minutes: 20)
    tally = Recipe.tallies([soup]).first
    [tally.shortlist, tally.timing.span]
  end

  # Rails: [5, 25, 20, true, false]
  def self.struct_block_methods
    stew = Recipe.create!(title: "stew", steps: "chop", prep_minutes: 5, cook_minutes: 20)
    range = stew.minutes_range
    [range.low, range.high, range.span, range.cover?(10), range.cover?(30)]
  end

  # A model nested in a module: Rails: 21
  def self.namespaced_model_value_classes
    Admin::Menu.priced(3, 4)
  end

  # A value class named JSON, and a nested class JSON, beside the model's
  # own code: the models load. Rails: ["A", "b!"]
  def self.json_named_classes
    [Memo::JSON.new("a").label, Ticket::JSON.shout("b")]
  end

  def self.run
    check("json_named_classes", json_named_classes == ["A", "b!"])
    check("data_block_methods", data_block_methods == [["boil", "salt", "water"], 4, "soup: 4 words, 3 distinct"])
    check("data_block_reads_the_model_scope", data_block_reads_the_model_scope == [["boil", "salt"], 15])
    check("namespaced_model_value_classes", namespaced_model_value_classes == 21)
    check("struct_block_methods", struct_block_methods == [5, 25, 20, true, false])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "recipes", force: :cascade do |t|
    t.string "title"
    t.string "steps"
    t.integer "prep_minutes"
    t.integer "cook_minutes"
  end
  create_table "menus", force: :cascade do |t|
    t.string "name"
  end
  create_table "memos", force: :cascade do |t|
    t.string "body"
  end
  create_table "tickets", force: :cascade do |t|
    t.string "body"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_models_value_class_block_methods_answer_as_rails_does() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_models_value_class_block_methods_answer_as_rails_does_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

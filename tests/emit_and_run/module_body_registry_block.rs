//! A module body statement that hands a block to another app class's
//! method, which runs the block on a builder of its own
//! (`instance_exec`), runs as the module loads: directly, and in a loop
//! over that class's constant, the method inherited from a base class, as
//! Ruby runs the body. Ignored as a fork spec until the compiler does;
//! the expected values are Ruby's answers (run under ActiveRecord 8.1.4).

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/recipe_builder.rb", r##"class RecipeBuilder
  attr_reader :id, :steps

  def initialize(id)
    @id = id
    @steps = []
  end

  def step(text)
    @steps << text
  end
end
"##)
        .write("app/models/base_catalog.rb", r##"class BaseCatalog
  def self.entries
    @entries ||= {}
  end

  def self.recipe(id, &block)
    builder = RecipeBuilder.new(id)
    builder.instance_exec(&block)
    entries[id] = builder
  end
end
"##)
        .write("app/models/kitchen_catalog.rb", r##"class KitchenCatalog < BaseCatalog
  DRINKS = %i[tea cocoa].freeze
end
"##)
        .write("app/models/recipes.rb", r##"module Recipes
  KitchenCatalog.recipe(:toast) do
    step "slice"
    step "heat"
  end

  KitchenCatalog::DRINKS.each do |drink|
    KitchenCatalog.recipe(drink) do
      step "boil"
      step "pour #{drink}"
    end
  end

  LOADED = true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    check("the module loaded", Recipes::LOADED)
    check("each entry registered", KitchenCatalog.entries.keys == [:toast, :tea, :cocoa])
    check("the block ran on the builder", KitchenCatalog.entries[:toast].steps == ["slice", "heat"])
    check("the loop's blocks too", KitchenCatalog.entries[:cocoa].steps == ["boil", "pour cocoa"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: ruby semantics"]
fn module_body_block_handed_to_another_class_runs() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn module_body_block_handed_to_another_class_runs_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

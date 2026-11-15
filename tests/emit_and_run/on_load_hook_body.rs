//! An `ActiveSupport.on_load` hook for a name a model fires with
//! `run_load_hooks(:name, self)` runs its whole block on that class: the
//! `class_attribute` it declares, the methods it defines and a reopening
//! `class_eval`, as Rails' lazy load hooks run it, whether the hook is
//! registered before the model loads or after. Ignored as a fork spec
//! until the compiler does; the expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/kite.rb", r##"class Kite < ApplicationRecord
  def label
    "kite #{name}"
  end

  ActiveSupport.run_load_hooks(:kite, self)
end
"##)
        .write("config/initializers/kite_extensions.rb", r##"ActiveSupport.on_load(:kite) do
  class_attribute :tail_color, default: "red"

  def tail
    "#{tail_color} tail on #{label}"
  end

  class_eval do
    def self.colored(color)
      where(color: color)
    end
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    k = Kite.create!(name: "box", color: "blue")
    Kite.create!(name: "delta", color: "green")
    # Rails: "red", "red tail on kite box"
    check("class_attribute from the hook", Kite.tail_color == "red")
    check("method from the hook", k.tail == "red tail on kite box")
    Kite.tail_color = "white"
    # Rails: "white tail on kite box"
    check("class_attribute writer", k.tail == "white tail on kite box")
    # Rails: ["box"]
    check("class method from the hook", Kite.colored("blue").pluck(:name) == ["box"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "kites", force: :cascade do |t|
    t.string "name"
    t.string "color"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: ruby semantics"]
fn an_on_load_hook_runs_its_whole_block_on_the_model() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn an_on_load_hook_runs_its_whole_block_on_the_model_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

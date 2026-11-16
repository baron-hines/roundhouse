//! An `ActiveSupport.on_load` hook for a name a model fires with
//! `run_load_hooks(:name, self)` runs its whole block on that class: the
//! `class_attribute` it declares, the methods it defines and a reopening
//! `class_eval`, as Rails' lazy load hooks run it, whether the hook is
//! registered before the model loads or after. The expected values are
//! ActiveRecord 8.1.4's answers.

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

/// A hook block that takes a block parameter (Rails' `class_eval` passes it
/// the class) is not one this splice binds. It is reported as an error
/// naming the hook and the model that fires it, never dropped in silence.
fn hook_with_a_block_parameter_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/kite.rb", r##"class Kite < ApplicationRecord
  ActiveSupport.run_load_hooks(:tether, self)
end
"##)
        .write("config/initializers/tether.rb", r##"ActiveSupport.on_load(:tether) do |base|
  base.class_attribute :tether_length
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "kites", force: :cascade do |t|
    t.string "name"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn an_on_load_hook_runs_its_whole_block_on_the_model() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

/// The hook's first statement (`class_attribute`) starts at the same offset
/// in the initializer (34) as the model's `self.table_name =` does in the
/// model (indented three spaces to land there): the spliced statement is
/// still the hook's, not taken for the model's.
#[test]
fn a_hook_statement_at_a_model_declarations_offset_still_runs() {
    let run = app()
        .edit("app/models/kite.rb", "class Kite < ApplicationRecord\n", "class Kite < ApplicationRecord\n   self.table_name = \"kites\"\n")
        .run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_on_load_hook_runs_its_whole_block_on_the_model_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
fn a_hook_block_the_splice_refuses_is_reported_naming_the_hook_and_the_model() {
    let errors = hook_with_a_block_parameter_app().check_errors();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    let error = &errors[0];
    assert!(error.contains(":tether"), "{error}");
    assert!(error.contains("Kite"), "{error}");
}

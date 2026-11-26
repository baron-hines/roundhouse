//! `a&.b` calls `b` when `a` is `false`, as Ruby's safe navigation guards
//! only `nil`; a `nil` receiver still answers `nil`. Ignored as a fork
//! spec until the compiler does; the expected values are ActiveRecord
//! 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/lamp.rb", r##"class Lamp < ApplicationRecord
  def state_word
    lit&.to_s
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    # Rails: "false", "true", nil
    check("false receiver", Lamp.new(lit: false).state_word == "false")
    check("true receiver", Lamp.new(lit: true).state_word == "true")
    check("nil receiver", Lamp.new(lit: nil).state_word.nil?)
    off = Lamp.create!(lit: false)
    # Rails: "false"
    check("loaded false receiver", Lamp.find(off.id).state_word == "false")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "lamps", force: :cascade do |t|
    t.boolean "lit"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: ruby semantics"]
fn safe_navigation_calls_through_a_false_receiver() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn safe_navigation_calls_through_a_false_receiver_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

//! A class-body loop over literal names that defines class methods
//! (`define_singleton_method`, `singleton_class.define_method`, or
//! `define_method` inside `class << self`) defines each one, so the class
//! methods written below it can call them, as Ruby runs the body top to
//! bottom. The expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/ticket.rb", r##"class Ticket < ApplicationRecord
  LANES = %w[front back].freeze

  LANES.each do |lane|
    define_singleton_method("#{lane}_count") { where(lane: lane).count }
    self.define_singleton_method("#{lane}?") { |ticket| ticket.lane == lane }
  end

  %i[open shut].each do |state|
    singleton_class.define_method("#{state}_ids") { where(state: state.to_s).order(:id).pluck(:id) }
  end

  class << self
    %w[front back].each do |lane|
      define_method("first_#{lane}") { where(lane: lane).order(:id).first }
    end
  end

  def self.tally
    "#{front_count}/#{back_count}"
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    a = Ticket.create!(lane: "front", state: "open")
    b = Ticket.create!(lane: "back", state: "shut")
    c = Ticket.create!(lane: "back", state: "open")
    # Rails: 1, 2, "1/2"
    check("define_singleton_method", Ticket.front_count == 1 && Ticket.back_count == 2)
    # Rails: true, false
    check("self.define_singleton_method", Ticket.front?(a) && !Ticket.back?(a))
    check("class def below the loop", Ticket.tally == "1/2")
    # Rails: [a.id, c.id], [b.id]
    check("singleton_class.define_method", Ticket.open_ids == [a.id, c.id] && Ticket.shut_ids == [b.id])
    # Rails: a, b
    check("define_method in class << self", Ticket.first_front == a && Ticket.first_back == b)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "tickets", force: :cascade do |t|
    t.string "lane"
    t.string "state"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_class_body_loop_defines_its_class_methods() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

/// Over a value the compile cannot list, each class-method loop is not
/// unrolled, and `check` reports it rather than letting its methods go
/// missing.
#[test]
fn a_class_method_loop_over_names_known_only_at_load_is_a_check_error() {
    for (find, replace, call) in [
        ("  LANES.each do |lane|\n", "  LANES.dup.each do |lane|\n", "define_singleton_method"),
        ("  %i[open shut].each do |state|\n", "  %i[open shut].dup.each do |state|\n", "singleton_class.define_method"),
    ] {
        let errors = app().edit("app/models/ticket.rb", find, replace).check_errors();
        assert_eq!(errors.len(), 1, "{find}: {errors:#?}");
        assert!(errors[0].contains(&format!("runs `{call}` in a shape the compile could not unroll")) && errors[0].contains("`Ticket`"), "{find}: {errors:#?}");
    }
    // In a block callback they are the record's own methods, not the
    // class's: no unroll error.
    let errors = app()
        .edit("app/models/ticket.rb", "  def self.tally\n", "  after_initialize do\n    define_singleton_method(:pinned?) { true }\n    singleton_class.define_method(:flagged?) { false }\n  end\n\n  def self.tally\n")
        .check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_class_body_loop_defines_its_class_methods_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

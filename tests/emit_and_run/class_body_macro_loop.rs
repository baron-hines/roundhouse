//! A class-body loop over literal names that runs method-defining macros
//! (`scope`, `define_method`) defines each method, so the `def`s written
//! below it can call them, as Ruby runs the body top to bottom (the `def`s
//! themselves already stand here). Ignored as a fork spec until the compiler does; the expected
//! values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/parcel.rb", r##"class Parcel < ApplicationRecord
  %w[small large].each do |size|
    scope size, -> { where(size: size) }
    define_method("#{size}?") { self.size == size }
  end

  def heavy?
    large? && weight > 10
  end

  def self.heavy
    large.where("weight > ?", 10)
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    a = Parcel.create!(size: "small", weight: 20)
    b = Parcel.create!(size: "large", weight: 20)
    Parcel.create!(size: "large", weight: 5)
    # Rails: [a.id], 2, false, true
    check("loop scope", Parcel.small.pluck(:id) == [a.id] && Parcel.large.count == 2)
    check("instance def below the loop", !a.heavy? && b.heavy?)
    # Rails: [b.id]
    check("class def below the loop", Parcel.heavy.pluck(:id) == [b.id])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "parcels", force: :cascade do |t|
    t.string "size"
    t.integer "weight"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: ruby semantics"]
fn a_class_body_loop_defines_its_scopes_and_methods() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn a_class_body_loop_defines_its_scopes_and_methods_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

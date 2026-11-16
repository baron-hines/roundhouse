//! A class-body loop over literal names that runs method-defining macros
//! (`scope`, `define_method`) defines each method, so the `def`s written
//! below it can call them, as Ruby runs the body top to bottom (the `def`s
//! themselves already stand here). The expected values are ActiveRecord
//! 8.1.4's answers.

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
fn a_class_body_loop_defines_its_scopes_and_methods() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

/// Over names known only when the class loads, the loop is not unrolled,
/// and `check` reports it rather than letting its methods go missing.
#[test]
fn a_loop_over_names_known_only_at_load_is_a_check_error() {
    let errors = app()
        .edit("app/models/parcel.rb", "  %w[small large].each do |size|\n", "  def self.sizes\n    %w[small large]\n  end\n\n  sizes.each do |size|\n")
        .check_errors();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].contains("could not unroll") && errors[0].contains("`Parcel`"), "{errors:#?}");
    // A receiverless block that runs them is not unrolled either, nor a `while` loop.
    for (find, replace) in [
        ("  %w[small large].each do |size|\n", "  each do |size|\n"),
        ("  %w[small large].each do |size|\n", "  size = \"small\"\n  while size\n"),
    ] {
        let errors = app().edit("app/models/parcel.rb", find, replace).check_errors();
        assert_eq!(errors.len(), 1, "{replace}: {errors:#?}");
        assert!(errors[0].contains("could not unroll"), "{replace}: {errors:#?}");
    }
}

/// `scope :"#{size}_ones"`: an interpolated Symbol name unrolls to each
/// element's Symbol, as a String name does. Rails: 1, 2.
#[test]
fn a_loop_scope_named_by_an_interpolated_symbol_is_defined() {
    let run = app()
        .edit("app/models/parcel.rb", "    scope size, -> { where(size: size) }\n", "    scope size, -> { where(size: size) }\n    scope :\"#{size}_ones\", -> { where(size: size) }\n")
        .run_ruby("Parcel.create!(size: \"small\", weight: 1)\n2.times { Parcel.create!(size: \"large\", weight: 1) }\nraise \"interpolated symbol scope\" unless Parcel.small_ones.count == 1 && Parcel.large_ones.count == 2\nputs \"probe passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_class_body_loop_defines_its_scopes_and_methods_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

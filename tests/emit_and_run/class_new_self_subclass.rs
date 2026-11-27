//! A class method that calls a bare `new`, inherited by an anonymous
//! `Class.new(self)` subclass, builds an instance of that subclass, as
//! Ruby's `self` is the class it is called on. Ignored as a fork spec
//! until the compiler does; the expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/lantern.rb", r##"class Lantern
  def self.lit
    new
  end

  def self.variant
    Class.new(self)
  end

  def glow
    "dim"
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    variant = Lantern.variant
    lit = variant.lit
    # Rails: true, true, "dim", true
    check("instance of the subclass", lit.class == variant && lit.class != Lantern)
    check("is a Lantern", lit.is_a?(Lantern))
    check("inherited method", lit.glow == "dim")
    check("base", Lantern.lit.class == Lantern)
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: ruby semantics"]
fn a_bare_new_inherited_by_an_anonymous_subclass_builds_that_subclass() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn a_bare_new_inherited_by_an_anonymous_subclass_builds_that_subclass_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

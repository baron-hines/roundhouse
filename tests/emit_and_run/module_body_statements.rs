//! A module body that defines a class and then runs statements on
//! constants (a call chain on a constant it just assigned, a constant
//! built from the class above) keeps those statements and runs them with
//! the class already defined, as Ruby runs the module body in order.
//! Ignored as a fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/palette.rb", r##"module Palette
  class Swatch
    attr_reader :name

    def initialize(name)
      @name = name
    end
  end

  SWATCHES = []
  SWATCHES.push(Swatch.new("teal"))
  SWATCHES.concat(%w[rust sand].map { |n| Swatch.new(n) })
  DEFAULT = SWATCHES.first
  NAMES = SWATCHES.map(&:name).freeze
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    # Rails: ["teal", "rust", "sand"], "teal", true
    check("statements on a constant", Palette::SWATCHES.map(&:name) == %w[teal rust sand])
    check("constant from the class", Palette::DEFAULT.name == "teal")
    check("derived constant", Palette::NAMES == %w[teal rust sand] && Palette::NAMES.frozen?)
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: ruby semantics"]
fn a_module_body_runs_its_statements_after_the_class_it_defines() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn a_module_body_runs_its_statements_after_the_class_it_defines_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

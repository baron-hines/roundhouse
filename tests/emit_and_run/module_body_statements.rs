//! A module body that defines a class and then runs statements on
//! constants (a call chain on a constant it just assigned, a constant
//! built from the class above) keeps those statements and runs them with
//! the class already defined, as Ruby runs the module body in order.
//! The expected values are ActiveRecord 8.1.4's answers.

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
fn a_module_body_runs_its_statements_after_the_class_it_defines() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_module_body_runs_its_statements_after_the_class_it_defines_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

/// A class from a namespace further out (`Catalog::Swatch`, read as `Swatch`
/// inside `Catalog::Palette`) is not one the emitted body requires, so a
/// call building it is not written back to run at load (where it would
/// raise NameError if the palette loaded first): it stays a reported drop.
#[test]
fn a_call_on_a_class_from_an_outer_namespace_is_not_written_back() {
    let catalog = || {
        emit_and_run::empty_app()
            .write("app/models/catalog/palette.rb", "module Catalog\n  module Palette\n    ITEMS = []\n    ITEMS.push(Swatch.new(\"teal\"))\n  end\nend\n")
            .write("app/models/catalog/swatch.rb", "module Catalog\n  class Swatch\n    def initialize(name)\n      @name = name\n    end\n  end\nend\n")
    };
    let (tree, _errors) = catalog().emit(roundhouse::project::BuildTarget::Ruby);
    let text = std::fs::read_to_string(tree.join("app/models/catalog/palette.rb")).unwrap();
    assert!(text.contains("ITEMS = []") && !text.contains("Swatch.new"), "{text}");
    let warnings = catalog().emit_warnings(roundhouse::project::BuildTarget::Ruby);
    assert!(
        warnings.iter().any(|w| w.message.contains("`push` in the body of `Catalog::Palette` is not modelled and is dropped")),
        "{warnings:#?}"
    );
    // Nor when both a nested and a top-level `Swatch` exist: the bare name
    // written back could reach the top-level one first.
    let (tree, _errors) = emit_and_run::empty_app()
        .write("app/models/palette.rb", "module Palette\n  ITEMS = []\n  ITEMS.push(Swatch.new)\nend\n")
        .write("app/models/palette/swatch.rb", "module Palette\n  class Swatch\n  end\nend\n")
        .write("app/models/swatch.rb", "class Swatch\nend\n")
        .emit(roundhouse::project::BuildTarget::Ruby);
    let text = std::fs::read_to_string(tree.join("app/models/palette.rb")).unwrap();
    assert!(text.contains("ITEMS = []") && !text.contains("Swatch.new"), "{text}");
}

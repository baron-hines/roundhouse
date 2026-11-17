//! A class or module whose declared name starts with `::` is defined at
//! the top level, wherever the declaration is written: `class ::Shelf::Box`
//! inside `module Adapter` defines `Shelf::Box`, and a class declared in the
//! body of `module ::Crate` is `Crate::<name>`. The expected values are
//! Ruby 4.0.7's answers for the same two files.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/shelf.rb", "module Shelf\nend\n")
        .write("app/models/adapter.rb", r##"module Adapter
  class ::Shelf::Box
    def self.kind
      "box"
    end
  end

  module ::Crate
    class Lid
      def self.kind
        "lid"
      end
    end
  end

  class Plug
    def self.kind
      "plug"
    end
  end
end
"##)
        .write("db/schema.rb", "ActiveRecord::Schema.define do\nend\n")
}

const SCRIPT: &str = r##"Adapter
answers = [
  defined?(Shelf::Box), defined?(Adapter::Shelf), Shelf::Box.kind,
  defined?(Crate::Lid), defined?(Adapter::Crate), Crate::Lid.kind,
  Adapter::Plug.kind,
]
expected = ["constant", nil, "box", "constant", nil, "lid", "plug"]
raise "rooted declarations: #{answers.inspect}" unless answers == expected
puts "probe passed"
"##;

#[test]
fn a_rooted_declaration_inside_a_module_is_defined_at_the_top_level() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_rooted_declaration_inside_a_module_is_defined_at_the_top_level_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

//! A plain class's macro that `class_eval`s a String per name (Warden's `Config.hash_accessor`):
//! the methods the String defines are the class's, as Ruby makes them when the class body runs.

use super::emit_and_run;

const KNOBS: &str = r#"class Knobs
  def self.knob(*names)
    names.each do |name|
      class_eval <<-METHOD, __FILE__, __LINE__ + 1
        def #{name}
          @values[:#{name}]
        end

        def #{name}=(value)
          @values[:#{name}] = value
        end
      METHOD
    end
  end

  knob :volume, :tone

  def initialize
    @values = {}
  end
end
"#;

const SCRIPT: &str = r#"
knobs = Knobs.new
knobs.volume = 3
knobs.tone = 7
raise "volume #{knobs.volume}" unless knobs.volume == 3
raise "tone #{knobs.tone}" unless knobs.tone == 7
puts "knobs passed"
"#;

#[test]
fn a_class_macro_that_class_evals_a_string_defines_its_methods_on_cruby() {
    let run = emit_and_run::real_blog().write("app/models/knobs.rb", KNOBS).run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("knobs passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_class_macro_that_class_evals_a_string_defines_its_methods_on_spinel() {
    let run = emit_and_run::real_blog().write("app/models/knobs.rb", KNOBS).run_spinel_slim(&[], SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("knobs passed"), "{}", run.stdout);
}

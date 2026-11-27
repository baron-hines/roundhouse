//! `alias_method :reading_without_scale, :reading` in a reopening of a class whose `reading` another file
//! defines (aasm's persistence/base.rb wrapping base.rb's `state`): the alias copies that one, and the
//! reopening's own `alias_method :reading, :reading_with_scale` then wraps it, as Ruby runs them.

use super::emit_and_run;

const GAUGE: &str = "class Gauge
  def initialize(value)
    @value = value
  end

  def reading
    @value
  end
end
";

const SCALE: &str = "require_relative \"gauge\"

class Gauge
  def reading_with_scale
    reading_without_scale * 10
  end
  alias_method :reading_without_scale, :reading
  alias_method :reading, :reading_with_scale
end
";

const SCRIPT: &str = r#"gauge = Gauge.new(4)
raise "reading #{gauge.reading}" unless gauge.reading == 40
raise "without #{gauge.reading_without_scale}" unless gauge.reading_without_scale == 4
puts "gauge passed"
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/gauge.rb", GAUGE).write("app/models/gauge_scale.rb", SCALE)
}

#[test]
fn an_alias_in_a_reopening_copies_the_method_another_file_defines_on_cruby() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("gauge passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_alias_in_a_reopening_copies_the_method_another_file_defines_on_spinel() {
    let run = app().run_spinel_slim(&[], SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("gauge passed"), "{}", run.stdout);
}

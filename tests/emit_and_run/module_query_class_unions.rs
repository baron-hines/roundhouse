//! A value that is one of several classes answers a Module query: a hook
//! each includer answers with its own class, or a local bound to
//! `flag ? A : B`. A write through such a value keeps the refusal.

use super::emit_and_run;
use roundhouse::project::BuildTarget;

const PROBE: &str = r#"module Shape
  def describe
    [shape::SIDES, shape.const_get(:LABEL)]
  end
end

class Circle
  SIDES = 0
  LABEL = "circle"
  include Shape

  private

  def shape
    Circle
  end
end

class Square
  SIDES = 4
  LABEL = "square"
  include Shape

  private

  def shape
    Square
  end
end

module Metric
  def metric?
    true
  end
end

class Celsius
  UNIT = "C"
  include Metric

  def freezing
    0
  end
end

class Fahrenheit
  UNIT = "F"
end

class Thermometer
  def unit_for(metric)
    scale = metric ? Celsius : Fahrenheit
    [scale::UNIT, scale.const_defined?(:UNIT)]
  end

  def traits_for(metric)
    scale = metric ? Celsius : Fahrenheit
    [scale.method_defined?(:freezing), scale.include?(Metric)]
  end
end
"#;

const ASSERTIONS: &str = r#"raise unless Circle.new.describe == [0, "circle"]
raise unless Square.new.describe == [4, "square"]
raise unless Thermometer.new.unit_for(true) == ["C", true]
raise unless Thermometer.new.unit_for(false) == ["F", true]
raise unless Thermometer.new.traits_for(true) == [true, true]
raise unless Thermometer.new.traits_for(false) == [false, false]
puts "class unions passed"
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/class_unions.rb", PROBE)
}

#[test]
fn queries_on_one_of_several_classes_run() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("class unions passed"), "{}", run.stdout);
}

#[test]
fn native_class_unions_agree() {
    let output = emit_and_run::ruby().arg("-e").arg(format!("{PROBE}\n{ASSERTIONS}")).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "class unions passed\n");
}

#[test]
fn strict_targets_keep_their_refusal() {
    let (_, errors) = app().emit(BuildTarget::Typescript);
    assert!(errors.iter().any(|e| e.contains("Module protocol") && e.contains("typescript")), "{errors:#?}");
    // Spinel compiles `shape::SIDES` and `scale::UNIT`, and answers
    // `method_defined?` and `include?` on one of several classes; it refuses
    // the constant queries there, which it does not answer.
    let (_, errors) = app().emit(BuildTarget::Spinel);
    let refused: Vec<_> = errors.iter().filter(|e| e.contains("on one of several classes")).collect();
    assert_eq!(refused.len(), 2, "{errors:#?}");
    assert!(refused.iter().any(|e| e.contains("`const_get`")) && refused.iter().any(|e| e.contains("`const_defined?`")),
        "{errors:#?}");
}

#[test]
fn a_value_that_may_not_be_a_class_stays_refused() {
    let shapes = |square: &str| format!(
        "module Shape\n def sides\n  shape::SIDES\n end\nend\nclass Circle\n SIDES = 0\n include Shape\n def shape = Circle\nend\nclass Square\n SIDES = 4\n include Shape\n{square}end\n"
    );
    for source in [
        shapes(" def shape = \"Square\"\n"),
        shapes(""),
        "class A\n UNIT = 1\nend\nclass Pick\n def go(flag)\n  x = flag ? A : \"A\"\n  x::UNIT\n end\nend\n".to_string(),
        "class A\nend\nclass B\nend\nclass Pick\n def go(flag)\n  x = flag ? A : B\n  x.define_method(:y) { 1 }\n end\nend\n".to_string(),
        "module Shape\n def go\n  shape.define_method(:y) { 1 }\n end\nend\nclass Circle\n include Shape\n def shape = Circle\nend\nclass Square\n include Shape\n def shape = Square\nend\n".to_string(),
    ] {
        let (_, errors) = emit_and_run::real_blog()
            .write("app/models/class_unions.rb", &source).emit(BuildTarget::Ruby);
        assert!(errors.iter().any(|e| e.contains("Module protocol")), "{source}: {errors:#?}");
    }
}

#[test]
fn a_copy_keeps_the_query_proof_only() {
    let source = "class Celsius\n  UNIT = \"C\"\nend\n\nclass Fahrenheit\n  UNIT = \"F\"\nend\n\nclass Thermometer\n  def copied_unit(metric)\n    scale = metric ? Celsius : Fahrenheit\n    copy = scale\n    copy.const_get(:UNIT)\n  end\nend\n";
    let run = emit_and_run::real_blog()
        .write("app/models/class_unions.rb", source)
        .run_ruby("raise unless [Thermometer.new.copied_unit(true), Thermometer.new.copied_unit(false)] == [\"C\", \"F\"]\nputs \"copy passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("copy passed"), "{}", run.stdout);
    // The copy is one of several classes, not one identity: a write through it stays refused.
    let write = source.replace("copy.const_get(:UNIT)", "copy.define_method(:y) { 1 }");
    let (_, errors) = emit_and_run::real_blog().write("app/models/class_unions.rb", &write).emit(BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("Module protocol")), "{errors:#?}");
}

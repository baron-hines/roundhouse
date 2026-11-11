//! `send("_#{kind}")` on `self` (Warden's `Hooks#_run_callbacks`): the methods of that shape the
//! receiver's class defines are the names it can call, so the send becomes a `case` over them.

use super::emit_and_run;

const BELLS: &str = r#"class Bells
  def ring(kind)
    send("_#{kind}")
  end

  def ring_with(kind, times)
    send("_#{kind}_times", times)
  end

  def _low
    "dong"
  end

  def _high
    "ding"
  end

  def _high_times(times)
    "ding" * times
  end
end
"#;

const SCRIPT: &str = r#"
bells = Bells.new
raise "low" unless bells.ring(:low) == "dong"
raise "high" unless bells.ring("high") == "ding"
raise "times" unless bells.ring_with(:high, 2) == "dingding"
refused = begin
  bells.ring(:missing)
  false
rescue StandardError
  true
end
raise "missing" unless refused
puts "bells passed"
"#;

#[test]
fn a_send_of_a_built_name_calls_the_classs_method_of_that_shape_on_cruby() {
    let run = emit_and_run::real_blog().write("app/models/bells.rb", BELLS).run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("bells passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_send_of_a_built_name_calls_the_classs_method_of_that_shape_on_spinel() {
    let run = emit_and_run::real_blog().write("app/models/bells.rb", BELLS).run_spinel_slim(&[], SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("bells passed"), "{}", run.stdout);
}

/// A module a model includes is not rewritten: the model may add a method of that shape (`_extra`),
/// which the module's `send` reaches and an arm list read from the module alone would miss.
#[test]
fn a_module_a_model_includes_keeps_its_send() {
    let (tree, errors) = emit_and_run::real_blog()
        .write("app/models/chimes.rb", "module Chimes\n  def chime(kind)\n    send(\"_#{kind}\")\n  end\n\n  def _soft\n    \"soft\"\n  end\nend\n")
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  include Chimes\n\n  def _extra\n    \"extra\"\n  end\n")
        .emit(roundhouse::build_target::BuildTarget::Ruby);
    let mut found = String::new();
    for entry in walk(&tree) {
        let text = std::fs::read_to_string(&entry).unwrap_or_default();
        if text.contains("def chime") {
            found = text;
        }
    }
    assert!(found.contains("send(\"_#{kind}\")"), "{found}\n{errors:#?}");
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else if path.extension().is_some_and(|e| e == "rb") {
            out.push(path);
        }
    }
    out
}

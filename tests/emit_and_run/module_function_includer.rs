//! `module_function` keeps a private instance copy, so a class that
//! includes the module calls it bare (parslet's `match` / `str` inside a
//! parser's rules, reached by lobsters' `/search`). Only the module
//! method used to survive ingest, so the bare call raised NoMethodError.

use super::emit_and_run;

const ATOMS: &str = r##"module Atoms
  module_function

  def match(pattern)
    "match(#{pattern})"
  end

  def str(text)
    "str(#{text})"
  end
end

class Rules
  include Atoms

  def word
    [match("[a-z]"), str("tag:")].join(" ")
  end
end
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/atoms.rb", ATOMS)
}

const ASSERTIONS: &str = r##"got = [Rules.new.word, Atoms.match("x"), Rules.new.respond_to?(:match)]
raise got.inspect unless got == ["match([a-z]) str(tag:)", "match(x)", false]
puts "module function includer passed"
"##;

#[test]
fn module_function_includer_runs() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("module function includer passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn module_function_includer_runs_on_spinel() {
    let run = app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("module function includer passed"), "{}", run.stdout);
}

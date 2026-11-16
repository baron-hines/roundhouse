//! A module that refines a class beside a nested constant keeps its
//! refinement, which is active in a class that calls `using` and nowhere
//! else, as Ruby defines and activates it. Ignored as a fork spec until
//! the compiler does; the expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/loud_text.rb", r##"module LoudText
  SUFFIX = "!"

  refine String do
    def loud
      upcase + SUFFIX
    end
  end
end
"##)
        .write("app/models/herald.rb", r##"class Herald
  using LoudText

  def announce(text)
    text.loud
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    # Rails: "HELLO!"
    check("active under using", Herald.new.announce("hello") == "HELLO!")
    # Rails: false
    check("inactive elsewhere", !"hello".respond_to?(:loud))
    check("nested constant", LoudText::SUFFIX == "!")
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: ruby semantics"]
fn a_refinement_is_active_only_under_using() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn a_refinement_is_active_only_under_using_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

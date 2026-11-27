//! A class body's `include(Module.new do ... end)` is kept when another
//! file's reopen of the class includes a module too: the instances answer
//! both modules' methods, as Ruby runs each file's include. Ignored as a
//! fork spec until the compiler does; the expected values are Ruby's
//! answers (run under ActiveRecord 8.1.4).

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/politeness.rb", r##"module Politeness
  def thank(name)
    "thanks, #{name}"
  end
end

class Porter
  include Politeness
end
"##)
        .write("app/models/porter.rb", r##"class Porter
  include(Module.new do
    def welcome(name)
      "welcome, #{name}"
    end
  end)

  def both(name)
    "#{welcome(name)}; #{thank(name)}"
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    porter = Porter.new
    check("the anonymous module's method", porter.welcome("lin") == "welcome, lin")
    check("the reopen's module's method", porter.thank("lin") == "thanks, lin")
    check("both from the class", porter.both("lin") == "welcome, lin; thanks, lin")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: ruby semantics"]
fn computed_include_beside_a_reopen_include_is_kept() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn computed_include_beside_a_reopen_include_is_kept_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

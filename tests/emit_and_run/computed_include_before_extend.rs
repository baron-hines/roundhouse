//! A class body's `include(Module.new do ... end)` followed by an
//! `extend` keeps its methods: the two mix into different chains, so the
//! instances answer the anonymous module's methods and the class answers
//! the extended module's, as Ruby runs both statements. Ignored as a fork
//! spec until the compiler does; the expected values are Ruby's answers
//! (run under ActiveRecord 8.1.4).

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/counting.rb", r##"module Counting
  def tally(items)
    items.size
  end
end
"##)
        .write("app/models/greeter.rb", r##"class Greeter
  include(Module.new do
    def greet(name)
      "hello #{name}"
    end
  end)
  extend(Counting)
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    check("the instances answer the anonymous module", Greeter.new.greet("ada") == "hello ada")
    check("the class answers the extend", Greeter.tally([1, 2, 3]) == 3)
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
fn computed_include_before_an_extend_is_kept() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn computed_include_before_an_extend_is_kept_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

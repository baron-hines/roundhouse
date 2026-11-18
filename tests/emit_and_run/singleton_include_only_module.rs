//! A module whose whole body is `class << self; include Formatting; end`
//! is a module of its own: Ruby extends it with Formatting, so
//! `Banner.shout` answers from the app module, as a body-level
//! `extend Formatting` would. Ignored as a fork spec until the compiler
//! does; the expected values are Ruby's answers (run under ActiveRecord
//! 8.1.4).

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/formatting.rb", r##"module Formatting
  def shout(text)
    "#{text.upcase}!"
  end

  def frame(text)
    "[#{text}]"
  end
end
"##)
        .write("app/models/banner.rb", r##"module Banner
  class << self
    include Formatting
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    check("the singleton include answers", Banner.shout("sale") == "SALE!")
    check("each of its methods", Banner.frame(Banner.shout("go")) == "[GO!]")
    check("Banner is extended with it", Banner.singleton_class.include?(Formatting))
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
fn module_with_only_a_singleton_include_is_a_module() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn module_with_only_a_singleton_include_is_a_module_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

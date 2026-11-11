//! `defined?(recv.m)` in a method, as Devise's `FailureApp#root_path_defined?` asks
//! `defined?(context.routes)`: "method" where the receiver answers a public `m`, nil where it does not, where `m` is
//! private, or where evaluating the receiver raises.

use super::emit_and_run;

const PROBE: &str = r#"class Routed
  def routes
    "routes"
  end

  def boom
    raise "boom"
  end

  def inner
    Routed.new
  end

  private

  def hidden
    "hidden"
  end
end

class Unrouted
end

class Probe
  def self.routes?(context)
    defined?(context.routes)
  end

  def self.hidden?(context)
    defined?(context.hidden)
  end

  def self.raising?(context)
    defined?(context.boom.routes)
  end

  def self.inner?(context)
    defined?(context.inner.routes)
  end
end
"#;

const SCRIPT: &str = r#"raise "routed #{Probe.routes?(Routed.new).inspect}" unless Probe.routes?(Routed.new) == "method"
raise "unrouted" unless Probe.routes?(Unrouted.new).nil?
raise "hidden" unless Probe.hidden?(Routed.new).nil?
raise "raising" unless Probe.raising?(Routed.new).nil?
raise "inner" unless Probe.inner?(Routed.new) == "method"
puts "defined method passed"
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/probe.rb", PROBE)
}

#[test]
fn defined_of_a_method_call_answers_as_ruby_does() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("defined method passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn defined_of_a_method_call_answers_as_ruby_does_natively() {
    let run = app().run_spinel(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("defined method passed"), "{}", run.stdout);
}

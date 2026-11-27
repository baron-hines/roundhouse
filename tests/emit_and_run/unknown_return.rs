//! A method returning an unknown value beside a typed one returns an
//! unknown, not the typed arm alone. parslet's `Atoms::Base#apply` is
//! `result = context.try_with_cache(...)` (an abstract `try` underneath)
//! then `return context.err_at(...)` or `return result`. Harvesting only
//! `err_at`'s `[bool, Cause?]` declared it in the RBS, and spinel raised
//! TypeError unboxing a successful parse's Array into a Cause.

use super::emit_and_run;

const STEPS: &str = r##"class Failure
  attr_reader :message

  def initialize(message)
    @message = message
  end
end

class Step
  def run(input)
    raise NotImplementedError, "#{self.class} must implement run"
  end
end

class Upcase < Step
  def run(input)
    [true, input.upcase]
  end
end

class Runner
  def apply(step, input, strict)
    result = step.run(input)
    return [false, Failure.new("empty")] if strict && input.empty?
    return result
  end
end
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/runner.rb", STEPS)
}

const ASSERTIONS: &str = r##"ok, value = Runner.new.apply(Upcase.new, "ab", true)
failed, failure = Runner.new.apply(Upcase.new, "", true)
got = [ok, value, failed, failure.message]
raise got.inspect unless got == [true, "AB", false, "empty"]
puts "unknown return passed"
"##;

#[test]
fn unknown_return_runs() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("unknown return passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn unknown_return_runs_on_spinel() {
    let run = app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("unknown return passed"), "{}", run.stdout);
}

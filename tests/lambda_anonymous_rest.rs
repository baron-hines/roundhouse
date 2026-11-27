//! A lambda that takes an anonymous rest (`->(*) { ... }`) accepts any
//! arguments. Ingest dropped the nameless `*` (a block's anonymous rest has
//! nothing to bind, and a proc ignores extra arguments), so the lambda was
//! emitted as `-> { ... }`, which raises ArgumentError for any argument.
//! A callback option such as `tags: ->(*) { [...] }` that a library invokes
//! as `tags.call(callee, args)` failed with "wrong number of arguments
//! (given 2, expected 0)".
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

const PROBE: &str = r##"module Lambdalab
  class Probe
    def self.tags
      ->(*) { %w[a b] }
    end

    def self.call_with(lambda, *args)
      lambda.call(*args)
    end

    def self.arities
      [tags.arity, ->(*rest) { rest }.arity]
    end

    def self.observe
      [
        call_with(tags),
        call_with(tags, :callee, [1]),
        call_with(->(*, last) { last }, 1, 2, 3),
        arities,
        tags.lambda?,
      ]
    end
  end
end
"##;

const OBSERVE: &str = "puts Lambdalab::Probe.observe.inspect\n";

#[test]
fn a_lambda_with_an_anonymous_rest_takes_any_arguments() {
    let expected = emit_and_run::ruby().arg("-e").arg(format!("{PROBE}\n{OBSERVE}")).output().expect("spawn ruby");
    assert!(expected.status.success(), "{}", String::from_utf8_lossy(&expected.stderr));
    let run = emit_and_run::empty_app()
        .write("app/models/lambdalab/probe.rb", PROBE)
        .run_ruby(&format!("require_relative \"app/models/lambdalab/probe\"\n{OBSERVE}"));
    run.assert_passes();
    assert_eq!(run.stdout, String::from_utf8_lossy(&expected.stdout));
}

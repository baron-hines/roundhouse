//! Ruby's exceptions outside StandardError (`SystemExit`, `Interrupt`,
//! `SyntaxError`, ...) and Encoding's conversion errors, raised and rescued
//! by their ancestors as gems do around `exit`, a signal or an `eval`. Ruby's
//! own on the ruby family, `lib/sp_exc.c` on spinel; the strict targets
//! report them.

use super::emit_and_run;

const GUARD: &str = r#"class Guard
  def self.trapped
    out = []
    begin
      raise Interrupt
    rescue SignalException => e
      out << e.class.name
    end
    begin
      exit 3
    rescue SystemExit => e
      out << e.status.to_s
      out << e.success?.to_s
    end
    begin
      raise SyntaxError, "bad"
    rescue ScriptError => e
      out << e.message
    end
    begin
      raise Encoding::UndefinedConversionError, "conv"
    rescue EncodingError => e
      out << e.message
    end
    begin
      raise SystemStackError, "deep"
    rescue Exception => e
      out << e.message
    end
    out
  end

  def self.bare
    begin
      raise SystemExit
    rescue
      "caught"
    end
  rescue SystemExit
    "passed through"
  end
end
"#;

fn guard_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/guard.rb", GUARD)
}

const ASSERTIONS: &str = r##"got = Guard.trapped
raise got.inspect unless got == ["Interrupt", "3", "false", "bad", "conv", "deep"]
raise Guard.bare.inspect unless Guard.bare == "passed through"
puts "core exceptions passed"
"##;

#[test]
fn core_exceptions_run() {
    let run = guard_app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("core exceptions passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn core_exceptions_run_on_spinel() {
    let run = guard_app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("core exceptions passed"), "{}", run.stdout);
}

#[test]
fn core_exceptions_are_reported_on_strict_targets() {
    use roundhouse::project::BuildTarget::*;
    for target in [Rust, Typescript, Go, Python, CSharp, Kotlin, Swift, Elixir, Crystal] {
        let (_emitted, errors) = guard_app().emit(target);
        assert!(errors.iter().any(|e| e.contains("SystemExit")), "{target:?}: {errors:#?}");
    }
}

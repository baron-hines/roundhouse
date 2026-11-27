//! `Signal`, `FileTest`, `Marshal` and `Fiber`, the core modules a gem's
//! process code reaches (a shutdown trap, a cache's Marshal round trip, a
//! file probe, a Fiber-stepped enumerator). Ruby's own on the ruby family,
//! spinel's runtime on spinel; the strict targets report them.

use super::emit_and_run;

const PROBE: &str = r#"class Probe
  def self.term_signal = Signal.list["TERM"]

  def self.trap_usr2
    Signal.trap("USR2") { }
    true
  end

  def self.round_trip(value) = Marshal.load(Marshal.dump(value))

  def self.dir?(path) = FileTest.directory?(path)

  def self.there?(path) = FileTest.exist?(path)

  def self.fiber_steps
    fiber = Fiber.new do
      Fiber.yield 1
      2
    end
    [fiber.resume, fiber.resume]
  end
end
"#;

fn probe_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/probe.rb", PROBE)
}

const ASSERTIONS: &str = r##"raise Probe.term_signal.inspect unless Probe.term_signal == 15
raise "trap" unless Probe.trap_usr2
got = Probe.round_trip(["a", 1])
raise got.inspect unless got == ["a", 1]
raise "dir" unless Probe.dir?(".") && !Probe.there?("no/such/path")
raise Probe.fiber_steps.inspect unless Probe.fiber_steps == [1, 2]
puts "process modules passed"
"##;

#[test]
fn process_modules_run() {
    let run = probe_app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("process modules passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn process_modules_run_on_spinel() {
    let run = probe_app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("process modules passed"), "{}", run.stdout);
}

#[test]
fn process_modules_are_reported_on_strict_targets() {
    use roundhouse::project::BuildTarget::*;
    for target in [Rust, Typescript, Go, Python, CSharp, Kotlin, Swift, Elixir, Crystal] {
        let (_emitted, errors) = probe_app().emit(target);
        for name in ["Signal", "FileTest", "Marshal", "Fiber"] {
            assert!(errors.iter().any(|e| e.contains(name)), "{target:?} {name}: {errors:#?}");
        }
    }
}

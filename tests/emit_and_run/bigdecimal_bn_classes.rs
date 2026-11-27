//! `BigDecimal` and `OpenSSL::BN` as class values: liquid's number filters ask
//! `result.is_a?(BigDecimal)`, a signature gem builds an `OpenSSL::BN`. The
//! libraries' own on the ruby family, spinel's packages on spinel; the strict
//! targets report them.

use super::emit_and_run;

const PROBE: &str = r#"require "bigdecimal"
require "openssl"

class NumProbe
  def self.decimal?(value) = value.is_a?(BigDecimal)

  def self.names = [BigDecimal, OpenSSL::BN].map(&:name)
end
"#;

fn probe_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/num_probe.rb", PROBE)
}

const ASSERTIONS: &str = r##"raise "decimal" unless NumProbe.decimal?(BigDecimal("1.5")) && !NumProbe.decimal?(1.5)
raise NumProbe.names.inspect unless NumProbe.names == ["BigDecimal", "OpenSSL::BN"]
puts "bigdecimal bn passed"
"##;

#[test]
fn bigdecimal_and_bn_classes_run() {
    let run = probe_app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("bigdecimal bn passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn bigdecimal_and_bn_classes_run_on_spinel() {
    let run = probe_app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("bigdecimal bn passed"), "{}", run.stdout);
}

#[test]
fn bigdecimal_and_bn_are_reported_on_strict_targets() {
    use roundhouse::project::BuildTarget::*;
    for target in [Rust, Typescript, Go, Python, CSharp, Kotlin, Swift, Elixir, Crystal] {
        let (_emitted, errors) = probe_app().emit(target);
        for name in ["BigDecimal", "OpenSSL::BN"] {
            assert!(
                errors.iter().any(|e| e.contains("bundled_constant not supported") && e.contains(&format!(": {name} "))),
                "{target:?} {name}: {errors:#?}",
            );
        }
    }
}

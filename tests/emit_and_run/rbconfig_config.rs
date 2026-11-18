//! `RbConfig::CONFIG`, the build configuration a gem branches on for the
//! platform (`host_os`, `DLEXT`). Ruby's own on the ruby family, spinel's
//! `builtins/rbconfig.rb` (the machine it compiled for) on spinel; the
//! strict targets report it.

use super::emit_and_run;

const PLATFORM: &str = r#"class Platform
  def self.os = RbConfig::CONFIG["host_os"]

  def self.mac? = RbConfig::CONFIG["host_os"].include?("darwin")

  def self.dlext = RbConfig::CONFIG.fetch("DLEXT", "so")
end
"#;

fn platform_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/platform.rb", PLATFORM)
}

const ASSERTIONS: &str = r##"raise Platform.os.inspect unless Platform.os.is_a?(String) && !Platform.os.empty?
raise Platform.mac?.inspect unless Platform.mac? == RUBY_PLATFORM.include?("darwin")
raise Platform.dlext.inspect unless ["so", "bundle"].include?(Platform.dlext)
puts "rbconfig passed"
"##;

#[test]
fn rbconfig_config_runs() {
    let run = platform_app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("rbconfig passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn rbconfig_config_runs_on_spinel() {
    let run = platform_app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("rbconfig passed"), "{}", run.stdout);
}

#[test]
fn rbconfig_config_is_reported_on_strict_targets() {
    use roundhouse::project::BuildTarget::*;
    for target in [Rust, Typescript, Go, Python, CSharp, Kotlin, Swift, Elixir, Crystal] {
        let (_emitted, errors) = platform_app().emit(target);
        assert!(errors.iter().any(|e| e.contains("RbConfig")), "{target:?}: {errors:#?}");
    }
}

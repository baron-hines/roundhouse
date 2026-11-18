//! `Logger::INFO` and `Logger::Severity::WARN`: the severities a gem sets or
//! compares a level with (lograge, sentry, logtail). The shared runtime's
//! Logger carries them, Ruby's values.

use super::emit_and_run;

const PROBE: &str = r#"class LogProbe
  def self.levels = [Logger::DEBUG, Logger::INFO, Logger::WARN, Logger::ERROR, Logger::FATAL, Logger::UNKNOWN]

  def self.quiet?(level) = level >= Logger::Severity::WARN
end
"#;

fn probe_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/log_probe.rb", PROBE)
}

const ASSERTIONS: &str = r##"raise LogProbe.levels.inspect unless LogProbe.levels == [0, 1, 2, 3, 4, 5]
raise "quiet" unless LogProbe.quiet?(Logger::ERROR) && !LogProbe.quiet?(Logger::INFO)
puts "logger severity passed"
"##;

#[test]
fn logger_severities_run() {
    let run = probe_app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("logger severity passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn logger_severities_run_on_spinel() {
    let run = probe_app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("logger severity passed"), "{}", run.stdout);
}

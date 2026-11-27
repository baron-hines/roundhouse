//!`Rails.env` and the stdlib/ActiveSupport calls a boot script makes on
//! it, against native Rails as the oracle: the environment inquirer
//! (`ActiveSupport::EnvironmentInquirer`, a String) compared,
//! interpolated and asked `local?`; `present?`/`blank?`/`presence`;
//! `File.dirname`; `warn`. The native control loads the Rails
//! `tests/support/rails_oracle.rb` picks (`RH_RAILS_ORACLE`).

#[path = "support/prerequisite.rs"]
mod prerequisite;
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use std::process::Command;

const ORACLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/rails_oracle.rb");

/// Whether the Rails oracle's activesupport is there; a run without it
/// says so and passes, as `docs/development/testing.md` asks. A probe that
/// cannot run (a broken oracle) fails rather than skips.
fn oracle_available() -> bool {
    let probe = Command::new("ruby")
        .args(["-e", &format!("require {ORACLE:?}\nputs RailsOracle.available?(\"activesupport\")")])
        .output()
        .expect("ruby");
    let answer = String::from_utf8_lossy(&probe.stdout).trim().to_string();
    assert!(probe.status.success() && (answer == "true" || answer == "false"), "oracle probe failed: {answer}\n{}", String::from_utf8_lossy(&probe.stderr));
    if answer == "false" {
        prerequisite::missing("the Rails oracle's activesupport is not available");
    }
    answer == "true"
}

/// App code, written once and run both natively and emitted. `ENV_OF`
/// builds the inquirer for a name: Rails' class natively, the
/// runtime's emitted.
const SURFACE: &str = r##"module Surface
  def self.env_report(env)
    parts = []
    parts << "test=#{env.test?} dev=#{env.development?} prod=#{env.production?} local=#{env.local?}"
    parts << "to_s=#{env} inspect=#{env.inspect} sym=#{env.to_sym.inspect}"
    parts << "eq=#{env == "test"} rev=#{"test" == env} sym_eq=#{env == :test}"
    parts << "case=#{case env when "test" then "t" when "production" then "p" else "o" end}"
    parts << "concat=#{"db_" + env}"
    parts.join(" | ")
  end

  def self.helpers_report
    parts = []
    parts << "present=#{"x".present?} #{"".present?} #{" ".present?} #{nil.present?} #{[].present?} #{{ a: 1 }.present?} #{0.present?} #{false.present?}"
    parts << "blank=#{" \t\n".blank?} #{nil.blank?} #{[1].blank?} #{{}.blank?} #{1.blank?} #{true.blank?}"
    parts << "presence=#{"x".presence.inspect} #{"".presence.inspect} #{nil.presence.inspect}"
    parts << "dirname=#{File.dirname("/a/b/c.rb")} #{File.dirname("c.rb")} #{File.dirname("/a/b/c.rb", 2)}"
    warn "surface warning"
    warn "two", "lines"
    parts.join(" | ")
  end
end
"##;

const REPORT: &str = r##"
$stderr = $stdout
%w[test development production staging].each { |name| puts Surface.env_report(ENV_OF.call(name)) }
puts Surface.helpers_report
begin
  ENV_OF.call("local")
rescue ArgumentError => e
  puts "local: #{e.message}"
end
"##;

const EXPECTED: &str = "\
test=true dev=false prod=false local=true | to_s=test inspect=\"test\" sym=:test | eq=true rev=true sym_eq=false | case=t | concat=db_test
test=false dev=true prod=false local=true | to_s=development inspect=\"development\" sym=:development | eq=false rev=false sym_eq=false | case=o | concat=db_development
test=false dev=false prod=true local=false | to_s=production inspect=\"production\" sym=:production | eq=false rev=false sym_eq=false | case=p | concat=db_production
test=false dev=false prod=false local=false | to_s=staging inspect=\"staging\" sym=:staging | eq=false rev=false sym_eq=false | case=o | concat=db_staging
surface warning
two
lines
present=true false false false false true true false | blank=true true false true false false | presence=\"x\" nil nil | dirname=/a/b . /a
local: 'local' is a reserved environment name
";

#[test]
fn native_rails_answers() {
    if !oracle_available() {
        return;
    }
    let output = Command::new("ruby")
        .args(["-e"])
        .arg(format!(
            "require {ORACLE:?}\nRailsOracle.activate(\"activesupport\")\nrequire \"active_support\"\nrequire \"active_support/environment_inquirer\"\nrequire \"active_support/core_ext/object/blank\"\n{SURFACE}\nENV_OF = ->(name) {{ ActiveSupport::EnvironmentInquirer.new(name) }}\n{REPORT}"
        ))
        .output()
        .expect("native Ruby control");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED);
}

#[test]
fn emitted_runtime_matches_native_rails() {
    let run = emit_and_run::real_blog()
        .write("app/lib/surface.rb", SURFACE)
        .run_ruby(&format!("ENV_OF = ->(name) {{ Rails::Env.new(name) }}\n{REPORT}"));
    run.assert_passes();
    assert_eq!(run.stdout, EXPECTED);
}

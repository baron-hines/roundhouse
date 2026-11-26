//! `prerequisite::missing`: a skip the run records, or a failure when the
//! run requires every prerequisite. Each test runs itself again as a child
//! with the environment under test; the child reports the missing
//! prerequisite.
#[path = "support/prerequisite.rs"]
mod prerequisite;
#[path = "support/test_subprocess.rs"]
mod test_subprocess;

use std::process::{Command, Output};

const CHILD: &str = "ROUNDHOUSE_PREREQUISITE_CHILD";
const WHAT: &str = "the probe gem is not installed";

/// In the child, report the missing prerequisite and say so.
fn as_child() -> bool {
    if std::env::var_os(CHILD).is_none() {
        return false;
    }
    prerequisite::missing(WHAT);
    println!("returned after the skip");
    true
}

fn child(test: &str, env: &[(&str, &str)]) -> Output {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env_remove(prerequisite::REQUIRE_VAR)
        .env_remove(prerequisite::LOG_VAR)
        .env(CHILD, "1")
        .envs(env.iter().copied())
        .output()
        .unwrap()
}

#[test]
fn a_skip_is_logged_with_its_test_and_reason() {
    if as_child() {
        return;
    }
    let test = test_subprocess::name!("a_skip_is_logged_with_its_test_and_reason");
    let log = std::env::temp_dir().join(format!("roundhouse-skip-log-{}", std::process::id()));
    std::fs::remove_file(&log).ok();
    let output = child(test, &[(prerequisite::LOG_VAR, log.to_str().unwrap())]);
    let logged = std::fs::read_to_string(&log).unwrap_or_default();
    std::fs::remove_file(&log).ok();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("returned after the skip"));
    assert!(String::from_utf8_lossy(&output.stderr).contains(&format!("skipping {test}: {WHAT}")));
    assert_eq!(logged, format!("{test}\t{WHAT}\n"));
}

#[test]
fn a_run_that_requires_prerequisites_fails_instead_of_skipping() {
    if as_child() {
        return;
    }
    let test = test_subprocess::name!("a_run_that_requires_prerequisites_fails_instead_of_skipping");
    let log = std::env::temp_dir().join(format!("roundhouse-required-log-{}", std::process::id()));
    std::fs::remove_file(&log).ok();
    let output = child(test, &[(prerequisite::REQUIRE_VAR, "1"), (prerequisite::LOG_VAR, log.to_str().unwrap())]);
    let logged = std::fs::read_to_string(&log).unwrap_or_default();
    std::fs::remove_file(&log).ok();
    assert_eq!(output.status.code(), Some(101), "{}", String::from_utf8_lossy(&output.stdout));
    assert_eq!(logged, format!("{test}\t{WHAT}\n"), "a required prerequisite is still named in the log");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("returned after the skip"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(&format!("{WHAT}; {} is set", prerequisite::REQUIRE_VAR)), "{stderr}");
}

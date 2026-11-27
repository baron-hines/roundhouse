//! The Db contract suite on the CRuby shims (runtime/spinel/db_cruby.rb
//! for SQLite, runtime/spinel/db_pg.rb on the pg gem for PostgreSQL), and
//! the lane table every shim is checked from.
//!
//! The suite, its layout and how to point a new shim at it are described
//! in tests/support/db_contract.rs. The Spinel lanes run from
//! tests/spinel_db_contract.rs (SQLite) and tests/spinel_pg_db.rs
//! (PostgreSQL), which need the Spinel toolchain.
//!
//!   cargo test --test db_contract
//!   ROUNDHOUSE_PG_TEST_URL=postgres://... cargo test --test db_contract
//!
//! A CRuby PostgreSQL lane skips, saying so, without ROUNDHOUSE_PG_TEST_URL,
//! and needs the pg gem with it.

#[path = "support/db_contract.rs"]
mod db_contract;

use db_contract::{Backend, LANES, Runtime, lanes, pg_test_url::schema_name};

#[test]
fn cruby_sqlite_shims_pass_the_contract() {
    let mut ran = 0;
    for lane in lanes(Runtime::Cruby, Backend::Sqlite) {
        db_contract::run_lane(lane);
        ran += 1;
    }
    assert!(ran > 0, "no CRuby SQLite lane");
}

#[test]
fn cruby_postgres_shims_pass_the_contract() {
    for lane in lanes(Runtime::Cruby, Backend::Postgres) {
        db_contract::run_lane(lane);
    }
}

/// Every Ruby-family `Db` shim the runtime ships is a lane, except the
/// JRuby one, which needs a JVM (compare-jruby covers it end to end).
#[test]
fn every_ruby_family_shim_has_a_lane() {
    let mut shims: Vec<String> = std::fs::read_dir("runtime/spinel")
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| (n == "db.rb" || n.starts_with("db_")) && n.ends_with(".rb"))
        .filter(|n| n != "db_jruby.rb")
        .map(|n| format!("runtime/spinel/{n}"))
        .collect();
    shims.sort();
    let mut laned: Vec<String> = LANES.iter().map(|l| l.shim.to_string()).collect();
    laned.sort();
    laned.dedup();
    assert_eq!(
        shims, laned,
        "a Db shim with no contract lane, or a lane with no shim"
    );
    for lane in LANES {
        assert!(
            lane.backend == Backend::Sqlite || lane.lane_file.is_some(),
            "the PostgreSQL lane {} names no lane file",
            lane.name
        );
    }
}

#[test]
fn pg_schema_names_are_identifiers_unique_per_call() {
    let a = schema_name("db contract::Widget-Lane");
    let b = schema_name("db contract::Widget-Lane");
    assert_ne!(a, b);
    assert!(a.starts_with("rh_db_contract__widget_lane_"), "{a}");
    let long = schema_name(&"gadget".repeat(40));
    assert!(long.len() + "_own".len() <= 63, "{long}");
    assert!(
        long.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    );
}

/// The lane deadline is enforced by the harness, with no timeout(1).
#[test]
#[should_panic(expected = "still running after 1s, killed")]
fn a_lane_that_hangs_is_killed_at_the_deadline() {
    db_contract::run_bounded(
        std::process::Command::new("ruby").args(["-e", "sleep 30"]),
        std::time::Duration::from_secs(1),
        "needs ruby",
    );
}

/// A program the lane cannot start names what the lane needs.
#[test]
#[should_panic(expected = "the lane needs a widget runner")]
fn a_missing_program_says_what_the_lane_needs() {
    db_contract::run_bounded(
        &mut std::process::Command::new("rh-no-such-widget-runner"),
        db_contract::LANE_DEADLINE,
        "the lane needs a widget runner",
    );
}

/// db_pg.rb's two drivers differ only through PgDriver, so the Spinel
/// file and its CRuby replacement define the same functions, and the
/// shim reaches the driver's session through it.
#[test]
fn both_pg_drivers_define_the_same_functions() {
    let functions = |path: &str| {
        let text = std::fs::read_to_string(path).unwrap();
        let mut names: Vec<String> = text
            .lines()
            .filter_map(|line| line.trim_start().strip_prefix("def self."))
            .map(|rest| rest.split(['(', ' ']).next().unwrap().to_string())
            .collect();
        names.sort();
        names
    };
    let spinel = functions("runtime/spinel/pg_driver.rb");
    assert_eq!(spinel, ["command_tag", "open", "sqlstate"]);
    assert_eq!(functions("runtime/spinel/pg_driver_cruby.rb"), spinel);
    let shim = std::fs::read_to_string("runtime/spinel/db_pg.rb").unwrap();
    assert!(shim.contains("require_relative \"pg_driver\""));
    assert!(!shim.contains("PG.connect("), "db_pg.rb opens sessions through PgDriver.open");
}

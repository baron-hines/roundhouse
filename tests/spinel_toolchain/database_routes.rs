//! An app that routes its models by role and shard, compiled natively
//! over two SQLite files on disk: each report printed by the binary is
//! Rails 8.1.4's for the same statements (`support/database_routes.rs`).

use super::database_routes_contract as contract;

fn section<'a>(stdout: &'a str, index: usize) -> &'a str {
    stdout.split("--\n").nth(index).unwrap_or("")
}

/// `connects_to shards:`: the two-file reduction, nested shard and role
/// blocks, restoration after a raise, a `return` and a `break`, the
/// write guard on every mutation, lazy missing pools, two threads on
/// opposite shards at once, and one lease across both files.
#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn shards_route_to_their_own_files_natively() {
    let run = contract::app(contract::SHARDS, &contract::shards_report())
        .run_spinel(&format!("{}{}", contract::BOOT, contract::SHARDS_SCRIPT));
    run.assert_passes();
    let expected = [
        contract::TWO_FILES_EXPECTED,
        contract::SHARDS_EXPECTED,
        contract::ERRORS_EXPECTED,
        contract::EXITS_EXPECTED,
        contract::GUARDS_EXPECTED,
        contract::ISOLATION_EXPECTED,
        contract::HANDLES_EXPECTED,
        contract::LEASE_EXPECTED,
    ];
    for (index, expected) in expected.iter().enumerate() {
        assert_eq!(
            contract::comparable(section(&run.stdout, index)),
            contract::comparable(expected),
            "section {index}\n{}\n{}",
            run.stdout,
            run.stderr
        );
    }
}

/// `connects_to database:` with a reading role: the write guard's
/// nesting, and a writing block on ActiveRecord::Base inside a reading
/// one, which ingest used to read as its body.
#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn roles_guard_writes_natively() {
    // The app's own class methods win over Rails' readers of the same
    // name, as in Ruby: one the connection class defines, and one from a
    // module a model extends. The report calls neither.
    let record = contract::roles_plain().replace("\nend", "\n\n  def self.default_shard\n    \"custom\"\n  end\nend");
    let run = contract::app(&record, &contract::roles_report())
        .write("app/models/titles.rb", "module Titles\n  def current_role\n    \"admin\"\n  end\nend\n")
        .write("app/models/note.rb", "class Note < ApplicationRecord\n  extend Titles\nend\n")
        .run_spinel_with_rbs(&format!(
            "{}RouteReport.roles\nputs Note.default_shard + \"!\"\nputs Note.current_role + \"!\"\n{}",
            contract::BOOT,
            contract::ONE_CONNECTION_SCRIPT
        ));
    run.assert_passes();
    let expected = format!("{}custom!\nadmin!\n{}", contract::ROLES_EXPECTED, contract::ONE_CONNECTION_EXPECTED);
    assert_eq!(contract::comparable(&run.stdout), contract::comparable(&expected), "{}", run.stderr);
}

fn assert_shard_report(script: &str, expected: &str) {
    let run = contract::app(contract::SHARDS, &contract::shards_report())
        .run_spinel(&format!("{}{script}", contract::BOOT));
    run.assert_passes();
    assert_eq!(contract::comparable(&run.stdout), contract::comparable(expected), "{}", run.stderr);
}

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn a_held_base_connection_preserves_model_routing() {
    assert_shard_report(contract::HELD_ROUTE_SCRIPT, contract::HELD_ROUTE_EXPECTED);
}

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn raw_transactions_refuse_cross_slot_switches() {
    assert_shard_report(contract::RAW_SWITCH_SCRIPT, contract::RAW_SWITCH_EXPECTED);
}

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn a_same_file_write_refreshes_the_held_reader_snapshot() {
    assert_shard_report(contract::SNAPSHOT_SCRIPT, contract::SNAPSHOT_EXPECTED);
}

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn writable_routes_create_missing_parent_directories() {
    let yml = contract::database_yml().replace("storage/right.sqlite3", "storage/missing/deep/right.sqlite3")
        .replace("left_reader:\n    adapter: sqlite3\n    database: storage/left.sqlite3", "left_reader:\n    adapter: sqlite3\n    database: storage/missing/deep/right.sqlite3");
    let run = contract::app(contract::SHARDS, &contract::shards_report())
        .write("config/database.yml", &yml)
        .run_spinel(&format!("{}ApplicationRecord.connected_to(shard: :right) {{ puts Note.count }}\nputs File.exist?(\"storage/missing/deep/right.sqlite3\")", contract::BOOT));
    run.assert_passes();
    assert_eq!(run.stdout, "0\ntrue\n");
}

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn writable_route_parents_tolerate_concurrent_creators() {
    assert_shard_report(contract::DIRECTORY_RACE_SCRIPT, contract::DIRECTORY_RACE_EXPECTED);
}

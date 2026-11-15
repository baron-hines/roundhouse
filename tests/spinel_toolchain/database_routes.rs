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
    let run = contract::app(&contract::roles_plain(), &contract::roles_report())
        .run_spinel_with_rbs(&format!("{}RouteReport.roles\n{}", contract::BOOT, contract::ONE_CONNECTION_SCRIPT));
    run.assert_passes();
    let expected = format!("{}{}", contract::ROLES_EXPECTED, contract::ONE_CONNECTION_EXPECTED);
    assert_eq!(contract::comparable(&run.stdout), contract::comparable(&expected), "{}", run.stderr);
}

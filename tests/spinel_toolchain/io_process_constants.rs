//! IO and process constants compiled natively: Spinel's runtime and its
//! `pty` and `shellwords` packages. The overlay twin is
//! `emit_and_run/io_process_constants.rs`.

use super::io_process_constants_contract as contract;

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn io_and_process_constants_run_as_ruby_runs_them_natively() {
    let run = contract::overlay().run_spinel(contract::SCRIPT);
    run.assert_passes();
    assert_eq!(run.stdout, contract::EXPECTED, "stderr:\n{}", run.stderr);
}

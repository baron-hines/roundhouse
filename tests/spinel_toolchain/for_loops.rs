//! The native half of `emit_and_run::for_loops`.

use super::for_loops_contract as contract;

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn for_loops_and_embedded_ivars_run_natively() {
    let run = contract::app().run_spinel(contract::ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("for loops passed"), "{}", run.stdout);
}

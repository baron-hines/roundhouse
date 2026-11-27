//! The native half of `emit_and_run::block_macros`.

use super::block_macros_contract as contract;

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn a_class_macro_given_a_block_runs_natively() {
    let run = contract::app().run_spinel(contract::ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("block macros passed"), "{}", run.stdout);
}

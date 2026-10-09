//! campfire's SQLite-observer caches compiled natively; the shared
//! contract and its CRuby twin are described in
//! `tests/support/campfire_caches.rs`.

use super::campfire_caches_contract::{self as contract, Contract};

fn assert_runs_natively(contract: &Contract) {
    let run = contract.overlay().run_spinel(contract.script);
    run.assert_passes();
    assert_eq!(run.stdout, contract.expected, "stderr:\n{}", run.stderr);
}

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn nested_multi_write_destructures_each_group_natively() {
    assert_runs_natively(&contract::NESTED_MULTI_WRITE);
}

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn data_define_block_methods_belong_to_the_data_class_natively() {
    assert_runs_natively(&contract::DATA_BLOCK_METHODS);
}

//! campfire's SQLite-observer caches on the CRuby overlay; the shared
//! contract and its native twin are described in
//! `tests/support/campfire_caches.rs`.

use super::campfire_caches_contract::{self as contract, Contract};

fn assert_runs(contract: &Contract) {
    let run = contract.overlay().run_ruby(contract.script);
    run.assert_passes();
    assert_eq!(run.stdout, contract.expected, "stderr:\n{}", run.stderr);
}

#[test]
fn nested_multi_write_destructures_each_group() {
    assert_runs(&contract::NESTED_MULTI_WRITE);
}

#[test]
fn data_define_block_methods_belong_to_the_data_class() {
    assert_runs(&contract::DATA_BLOCK_METHODS);
}

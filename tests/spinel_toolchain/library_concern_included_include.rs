//! The native half of `support/library_concern_included_include.rs`.

use super::library_concern_included_include_contract as shared;

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn a_library_class_takes_the_include_its_concern_runs_natively() {
    let run = shared::app().run_spinel(shared::ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("library concern include OK"), "{}", run.stdout);
}

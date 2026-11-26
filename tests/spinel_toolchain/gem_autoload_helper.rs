//! The native half of `emit_and_run::gem_autoload_helper`.

use super::gem_autoload_helper_contract as gem;

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn a_gem_registering_its_files_through_a_helper_runs_natively() {
    let run = gem::overlay().run_spinel(gem::ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("autoload helper OK"), "{}", run.stdout);
}

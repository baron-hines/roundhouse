//! The native half of `emit_and_run::auto_source_gem`.

use super::auto_source_gem_contract as auto_source_gem;

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn an_installed_gem_runs_natively_from_its_source() {
    let run = auto_source_gem::overlay().run_spinel(auto_source_gem::ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("auto source gem OK"), "{}", run.stdout);
}

/// The native half of `emit_and_run::auto_source_gem::a_gem_s_bare_read_of_a_constant_nested_elsewhere_is_ruby_s_name_error`.
#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn a_gem_s_bare_read_of_a_constant_nested_elsewhere_raises_name_error_natively() {
    let run = auto_source_gem::misnamed_overlay().run_spinel(auto_source_gem::MISNAMED_ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("misnamed gem constant OK"), "{}", run.stdout);
}

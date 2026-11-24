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

/// The native half of `emit_and_run::auto_source_gem::a_gem_file_s_top_level_prepend_is_module_s`: the gem shake keeps
/// the class's own method the prepended module's `super` reaches.
#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn a_gem_file_s_top_level_prepend_runs_natively() {
    let run = auto_source_gem::top_level_prepend_overlay().run_spinel(auto_source_gem::TOP_LEVEL_PREPEND_ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("top-level prepend OK"), "{}", run.stdout);
}

/// The native half of `emit_and_run::auto_source_gem::a_gem_s_constant_write_into_another_class_is_that_class_s`.
#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn a_gem_s_constant_write_into_another_class_runs_natively() {
    let run = auto_source_gem::top_level_constant_overlay().run_spinel(auto_source_gem::TOP_LEVEL_CONSTANT_ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("top-level constant OK"), "{}", run.stdout);
}

/// The native half of `emit_and_run::auto_source_gem::a_gem_s_platform_constant_read_under_its_test_is_typed`.
#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn a_gem_s_platform_constant_read_under_its_test_runs_natively() {
    let run = auto_source_gem::platform_constant_overlay().run_spinel(auto_source_gem::PLATFORM_CONSTANT_ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("platform constant OK"), "{}", run.stdout);
}

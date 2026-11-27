//! The Spinel twin of `tests/emit_and_run/class_param_or_assign.rs`.

use super::class_param_or_assign_app::{app, SCRIPT};

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn a_class_parameter_or_assigned_a_new_class_answers_module_queries_natively() {
    let run = app().run_spinel(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("class parameter passed"), "{}", run.stdout);
}

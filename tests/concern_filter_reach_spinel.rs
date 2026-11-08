//! The Spinel side of `concern_filter_reach`. Needs a Spinel compiler, so
//! it is `#[ignore]`d like its sibling suites; CI's `spinel-framework` job
//! runs it with `--ignored` (`scripts/ci-plan.py`'s `SPINEL_TESTS`).

#[path = "support/emit_and_run.rs"]
mod emit_and_run;
#[path = "support/concern_filter_reach_app.rs"]
mod concern_filter_reach_app;

#[test]
#[ignore = "requires the Spinel toolchain"]
fn concern_filters_reach_controllers_as_rails_registers_them_on_spinel() {
    let run = concern_filter_reach_app::app().run_spinel(concern_filter_reach_app::ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("concern filters reached"), "{}", run.stdout);
}

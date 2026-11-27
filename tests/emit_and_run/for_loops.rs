use super::for_loops_contract as contract;

#[test]
fn for_loops_and_embedded_ivars_run_after_emission() {
    contract::app().run_ruby(contract::ASSERTIONS).assert_passes();
}

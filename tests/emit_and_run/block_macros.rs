use super::block_macros_contract as contract;

#[test]
fn a_class_macro_given_a_block_runs_after_emission() {
    contract::app().run_ruby(contract::ASSERTIONS).assert_passes();
}

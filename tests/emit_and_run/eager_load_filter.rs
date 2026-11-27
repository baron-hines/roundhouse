use super::eager_load_filter_contract as contract;

#[test]
fn an_eager_load_filters_parents_and_their_association() {
    let run = contract::app().run_ruby(contract::ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("eager load passed"));
}

use super::data_record_siblings_contract as contract;

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn a_body_reader_override_runs_natively_without_changing_stored_members() {
    let run = contract::override_overlay("lib/artifact.rb").run_spinel_slim(&[], contract::OVERRIDE_ASSERTIONS);
    run.assert_passes();
    assert_eq!(run.stdout, contract::OVERRIDE_EXPECTED, "{}", run.stderr);
}

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn a_data_record_beside_a_module_runs_natively_with_its_signatures() {
    for path in ["lib/artifact.rb", "app/models/artifact.rb"] {
        let run = contract::overlay(path).run_spinel_slim(&[], contract::ASSERTIONS);
        run.assert_passes();
        assert_eq!(run.stdout, contract::EXPECTED, "{path}: {}", run.stderr);
    }
}

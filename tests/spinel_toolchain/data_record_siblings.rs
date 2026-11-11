use super::data_record_siblings_contract as contract;

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn data_composition_runs_natively_with_generated_signatures() {
    use std::process::Command;
    for (source, assertions, expected) in contract::COMPOSITION_CASES {
        let (tree, errors) = super::emit_and_run::real_blog()
            .write("lib/artifact.rb", source).emit(roundhouse::project::BuildTarget::Spinel);
        assert!(errors.is_empty(), "{errors:?}");
        std::fs::write(tree.join("driver.rb"), format!("require_relative \"app/models/artifact\"\n{assertions}")).unwrap();
        let output = Command::new(std::env::var("SPINEL").unwrap_or("spinel".into()))
            .args(["--strict-require", "--rbs", "app/models", "driver.rb", "-o", "driver"])
            .current_dir(&tree).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let output = Command::new(tree.join("driver")).current_dir(&tree).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), *expected);
    }
}

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

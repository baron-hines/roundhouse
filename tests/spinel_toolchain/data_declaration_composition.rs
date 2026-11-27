#[path = "../support/data_declaration_composition.rs"]
mod contract;
use super::emit_and_run;

#[test]
#[ignore = "requires Spinel"]
fn data_declarations_match_ruby_natively() {
    for (name, source, assertions, expected) in contract::CASES.iter().copied().chain(
        contract::REVIEW_CASES.iter().chain(contract::LITERAL_CASES).filter(|case| case.3 == "match").map(|case| (case.0, case.1, case.2, case.4))) {
        let (tree, errors) = contract::app(source).emit(roundhouse::project::BuildTarget::Spinel);
        assert!(errors.is_empty(), "{name}: {errors:?}");
        std::fs::write(tree.join("driver.rb"), format!("require_relative 'app/models'\n{assertions}")).unwrap();
        let compiled = std::process::Command::new(std::env::var("SPINEL").unwrap_or("spinel".into()))
            .args(["--strict-require", "--rbs", "app/models", "driver.rb", "-o", "driver"])
            .current_dir(&tree).output().unwrap();
        assert!(compiled.status.success(), "{name}: {}", String::from_utf8_lossy(&compiled.stderr));
        let output = std::process::Command::new(tree.join("driver")).current_dir(&tree).output().unwrap();
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{name}");
        println!("native parity: {name}");
    }
}

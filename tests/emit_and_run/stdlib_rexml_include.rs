use roundhouse::build_target::BuildTarget;

use super::stdlib_rexml_include_contract as contract;

#[test]
fn rexml_classes_named_through_include_run_after_emission() {
    contract::app().run_ruby(contract::ASSERTIONS).assert_passes();
}

/// A strict target has no REXML: each class is a `bundled_constant` gap
/// there, not an emitted reference to a class that does not exist.
#[test]
fn rexml_is_reported_where_no_runtime_has_it() {
    let (_emitted, errors) = contract::app().emit(BuildTarget::Typescript);
    for name in ["REXML::Document", "REXML::Comment", "REXML::CData", "REXML::DocType", "REXML::Formatters::Pretty"] {
        let want = format!("bundled_constant not supported (typescript): {name} is not available");
        assert!(errors.iter().any(|e| e.contains(&want)), "{name}: {errors:?}");
    }
}

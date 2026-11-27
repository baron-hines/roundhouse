//! FileUtils, GC, OpenSSL::HMAC, CSV::MalformedCSVError, Enumerator::Product,
//! Enumerator::Lazy, Enumerator::Chain and OpenStruct resolve, and the emitted program answers what
//! plain Ruby answers.

use super::stdlib_rnp_contract as contract;

#[test]
fn stdlib_constants_an_api_uses_execute_after_emission() {
    contract::overlay().run_ruby(contract::ASSERTIONS).assert_passes();
}

#[test]
fn open_struct_members_execute_after_emission() {
    contract::ostruct_overlay().run_ruby(contract::OSTRUCT_ASSERTIONS).assert_passes();
}

/// The other targets have none of these libraries: each constant is a
/// `bundled_constant` gap there, not an emitted reference to a class that
/// does not exist. Crystal stands in for them.
#[test]
fn strict_targets_ledger_the_stdlib_constants() {
    let (_tree, mut errors) = contract::overlay().emit(roundhouse::project::BuildTarget::Crystal);
    errors.extend(contract::ostruct_overlay().emit(roundhouse::project::BuildTarget::Crystal).1);
    for name in ["FileUtils", "GC", "OpenSSL::HMAC", "CSV::MalformedCSVError", "Enumerator::Product", "Enumerator::Lazy", "Enumerator::Chain", "OpenStruct"] {
        assert!(
            errors.iter().any(|e| e.contains(&format!("{name} is not available as a class/module value on crystal"))),
            "{name} not ledgered on Crystal: {errors:#?}"
        );
    }
}

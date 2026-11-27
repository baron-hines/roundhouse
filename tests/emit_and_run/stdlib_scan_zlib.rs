use super::stdlib_scan_zlib_contract as contract;

#[test]
fn string_scanner_math_pi_and_zlib_run_after_emission() {
    contract::app().run_ruby(contract::ASSERTIONS).assert_passes();
}

/// A strict target has neither library: `StringScanner` is a
/// `bundled_constant` gap there and `Zlib.deflate` a `bundled_method`.
#[test]
fn string_scanner_and_zlib_are_reported_where_no_runtime_has_them() {
    let (_emitted, errors) = contract::app().emit(roundhouse::build_target::BuildTarget::Typescript);
    assert!(errors.iter().any(|e| e.contains("bundled_constant not supported (typescript): StringScanner")), "{errors:?}");
    assert!(errors.iter().any(|e| e.contains("bundled_method not supported (typescript): Zlib.deflate")), "{errors:?}");
}

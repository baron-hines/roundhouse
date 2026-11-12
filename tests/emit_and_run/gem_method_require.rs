//! The Spinel emit of `support/gem_method_require.rs`; the ruby family keeps
//! loading the installed gem, so its lane has nothing of the gem's to emit.

use roundhouse::build_target::BuildTarget;

use super::gem_method_require_contract as contract;

/// The Spinel emit loads what Ruby loads: nothing until the method runs,
/// then the `LoadError` Ruby raises at the `require`. Spinel resolves a
/// `require` statically, and `spin build` refuses one it cannot satisfy
/// (`--strict-require`), so the `require` itself must not reach the tree.
#[test]
fn the_spinel_emit_raises_load_error_for_a_gems_method_body_require_of_an_absent_library() {
    let (emitted, errors) = contract::app().emit(BuildTarget::Spinel);
    assert!(!errors.iter().any(|e| e.contains("shelf") || e.contains("fancy_printer")), "{errors:#?}");
    let pretty = std::fs::read_to_string(emitted.join("app/models/shelf/pretty.rb")).expect("pretty.rb");
    assert!(!pretty.contains("require \"fancy_printer\""), "{pretty}");
    assert!(pretty.contains("raise LoadError.new(\"cannot load such file -- fancy_printer\")"), "{pretty}");
}

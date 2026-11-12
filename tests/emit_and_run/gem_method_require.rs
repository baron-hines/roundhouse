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

/// A library another locked, installed gem ships as `lib/optional_backend.rb` loads on Ruby, whatever that gem's
/// name or namespace: its `require` stays.
#[test]
fn the_spinel_emit_keeps_a_require_another_locked_gem_ships() {
    let (emitted, errors) = contract::app_with_backend().emit(BuildTarget::Spinel);
    assert!(!errors.iter().any(|e| e.contains("optional_backend")), "{errors:#?}");
    let backend = std::fs::read_to_string(emitted.join("app/models/shelf/backend.rb")).expect("backend.rb");
    assert!(!backend.contains("cannot load such file -- optional_backend"), "{backend}");
    assert!(backend.contains("require \"optional_backend\""), "{backend}");
}

/// A feature under a standard-library root (`net`) is the standard library's only where it ships that path:
/// `net/http` stays a `require`, and `net/http/persistent`, which only the absent net-http-persistent ships,
/// becomes Ruby's `LoadError`.
#[test]
fn the_spinel_emit_raises_load_error_for_an_absent_feature_under_a_standard_library_root() {
    let (emitted, errors) = contract::app_with_http().emit(BuildTarget::Spinel);
    assert!(!errors.iter().any(|e| e.contains("net/http")), "{errors:#?}");
    let http = std::fs::read_to_string(emitted.join("app/models/shelf/http.rb")).expect("http.rb");
    assert!(!http.contains("require \"net/http/persistent\""), "{http}");
    assert!(http.contains("raise LoadError.new(\"cannot load such file -- net/http/persistent\")"), "{http}");
    assert!(http.contains("require \"net/http\""), "{http}");
}

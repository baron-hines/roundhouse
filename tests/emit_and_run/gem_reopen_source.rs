//! A source gem's class that the app reopens keeps the gem's methods
//! (#2093). See `tests/support/gem_reopen_source.rs`.

use roundhouse::build_target::BuildTarget;

use super::gem_reopen_source_contract as contract;

/// The Ruby family, with the gem listed in `source_gems` so the tree
/// carries it: the gem's methods and the app's, the app's `label` winning.
#[test]
fn a_reopened_gem_class_answers_the_gems_methods_and_the_apps() {
    contract::listed(contract::local_app()).run_ruby(contract::LOCAL_ASSERTIONS).assert_passes();
}

#[test]
fn a_gem_class_reopened_for_a_constant_and_an_include_answers_all_three() {
    contract::listed(contract::extras_app()).run_ruby(contract::EXTRAS_ASSERTIONS).assert_passes();
}

/// The emitted `Shelf::Box`: one file holding each opening in load order.
fn shelf_box(app: super::emit_and_run::Overlay) -> String {
    let (emitted, _errors) = app.emit(BuildTarget::Spinel);
    std::fs::read_to_string(emitted.join("app/models/shelf/box.rb")).expect("app/models/shelf/box.rb")
        .lines()
        .filter(|l| !l.starts_with("#<SPINEL_SOURCE>"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// On Spinel the gem is compiled from source because the app names
/// `Shelf::Box`, even though the app's own reopen also declares it: the
/// gem's opening comes first and the app's after it, so the app's `label`
/// is the one that stands, as in Ruby.
#[test]
fn the_spinel_emit_compiles_the_gem_class_the_app_reopens() {
    let source = shelf_box(contract::local_app());
    let at = |needle: &str| source.find(needle).unwrap_or_else(|| panic!("{needle:?} in\n{source}"));
    let (kind, local) = (at("def self.kind"), at("def self.local_tag"));
    let gem_label = at("\"shelf box\"");
    let app_label = at("\"app box\"");
    assert!(kind < local && gem_label < app_label, "{source}");
}

/// A reopen that adds only a constant and an `include` brings the gem in too.
/// (Only what the app reaches is kept: `label`, which nothing calls, is not.)
#[test]
fn the_spinel_emit_compiles_the_gem_class_a_constant_only_reopen_names() {
    let source = shelf_box(contract::extras_app());
    for needle in ["def self.kind", "LIMIT = 3", "include Tagging"] {
        assert!(source.contains(needle), "{needle:?} in\n{source}");
    }
    assert!(source.find("def self.kind").unwrap() < source.find("LIMIT = 3").unwrap(), "{source}");
}

/// A reopen written in an initializer, run on the Ruby family with the gem
/// listed: the gem's methods and the initializer's, the initializer's
/// `label` winning.
#[test]
fn a_gem_class_reopened_in_an_initializer_answers_the_gems_methods_and_the_apps() {
    contract::listed(contract::initializer_app()).run_ruby(contract::LOCAL_ASSERTIONS).assert_passes();
}

/// On Spinel the initializer's opening is emitted after the gem's, in the
/// same file, in the nested form and in the compact one.
#[test]
fn the_spinel_emit_keeps_a_gem_class_reopened_in_an_initializer() {
    for app in [contract::initializer_app(), contract::compact_initializer_app()] {
        let source = shelf_box(app);
        let at = |needle: &str| source.find(needle).unwrap_or_else(|| panic!("{needle:?} in\n{source}"));
        let (kind, local) = (at("def self.kind"), at("def self.local_tag"));
        let gem_label = at("\"shelf box\"");
        let app_label = at("\"app box\"");
        assert!(kind < local && gem_label < app_label, "{source}");
    }
}

/// The `class_eval` form, run on the Ruby family with the gem listed.
#[test]
fn a_gem_class_class_evaled_in_an_initializer_answers_the_gems_methods_and_the_apps() {
    contract::listed(contract::class_eval_app()).run_ruby(contract::LOCAL_ASSERTIONS).assert_passes();
}

/// On Spinel a `class_eval` block of `def`s on a gem's class is one more
/// opening of it, after the gem's, and the initializer statement is no
/// longer reported as having no effect.
#[test]
fn the_spinel_emit_keeps_a_gem_class_class_evaled_in_an_initializer() {
    let unconsumed = |app: super::emit_and_run::Overlay| -> Vec<String> {
        app.ingest_diagnostics()
            .into_iter()
            .filter(|d| matches!(d.kind, roundhouse::diagnostic::DiagnosticKind::UnconsumedInitializer { .. }))
            .map(|d| d.message)
            .collect()
    };
    let source = shelf_box(contract::class_eval_app());
    let at = |needle: &str| source.find(needle).unwrap_or_else(|| panic!("{needle:?} in\n{source}"));
    let (kind, local) = (at("def self.kind"), at("def self.local_tag"));
    let gem_label = at("\"shelf box\"");
    let app_label = at("\"app box\"");
    assert!(kind < local && gem_label < app_label, "{source}");
    let warned = unconsumed(contract::class_eval_app());
    assert!(!warned.iter().any(|m| m.contains("Shelf::Box.class_eval")), "{warned:#?}");
}

/// A block with any statement besides `def`s is not taken: it stays
/// reported, and the emitted class is the gem's alone.
#[test]
fn a_class_eval_block_that_writes_a_constant_is_left_reported() {
    let source = shelf_box(contract::class_eval_limit_app());
    assert!(!source.contains("local_tag"), "{source}");
    let warned: Vec<String> = contract::class_eval_limit_app()
        .ingest_diagnostics()
        .into_iter()
        .filter(|d| matches!(d.kind, roundhouse::diagnostic::DiagnosticKind::UnconsumedInitializer { .. }))
        .map(|d| d.message)
        .collect();
    assert!(warned.iter().any(|m| m.contains("Shelf::Box.class_eval")), "{warned:#?}");
}

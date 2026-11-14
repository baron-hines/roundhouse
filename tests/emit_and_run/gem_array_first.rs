//! `first` on what a class's own `[]` answers is that value's method.
//! See `tests/support/gem_array_first.rs`.

use roundhouse::build_target::BuildTarget;

use super::gem_array_first_contract as contract;

/// The Ruby family runs the gem's `dn`: Ruby 4.0.7 answers `["cn=shelf", ""]`.
#[test]
fn a_gem_class_reading_first_of_its_own_index_answers_the_arrays_first() {
    contract::app().run_ruby(contract::ASSERTIONS).assert_passes();
}

/// On Spinel `self[:dn]` is the Array `[]` answers, so `first` stays the
/// Array's rather than becoming `ActiveSupport.string_first`, whose
/// String parameter the Array would contradict.
#[test]
fn the_spinel_emit_keeps_first_on_a_classs_own_index_as_the_arrays() {
    let (emitted, _errors) = contract::app().emit(BuildTarget::Spinel);
    let source = std::fs::read_to_string(emitted.join("app/models/shelf/entry.rb")).expect("app/models/shelf/entry.rb");
    assert!(!source.contains("string_first"), "{source}");
    assert!(source.contains("self[:dn].first"), "{source}");
}

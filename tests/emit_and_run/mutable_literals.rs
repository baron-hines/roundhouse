use roundhouse::build_target::BuildTarget;

use super::mutable_literals_contract as contract;

#[test]
fn a_literal_appended_to_in_a_file_without_the_magic_comment_runs_after_emission() {
    contract::app().run_ruby(contract::ASSERTIONS).assert_passes();
}

/// Only a file without the magic comment spells its held literals `+"…"`.
#[test]
fn the_magic_comment_keeps_a_held_literal_bare() {
    let (emitted, errors) = contract::app().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    let builder = std::fs::read_to_string(emitted.join("app/models/builder.rb")).expect("builder");
    assert!(builder.contains("s = +\"\""), "{builder}");
    let frozen = std::fs::read_to_string(emitted.join("app/models/frozen.rb")).expect("frozen");
    assert!(frozen.contains("s = \"kept\""), "{frozen}");
}

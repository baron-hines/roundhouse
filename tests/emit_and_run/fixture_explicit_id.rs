//! A fixture row that sets its own key (`id: 1001`) is that record,
//! as Rails' `FixtureSet` loads it: ActiveRecord 8.1.4.1 answers
//! `one.id = 1001` for the row, and the accessor finds it there. A row
//! without one keeps the id it is given otherwise.
//!
//! The loader stored the row under its own id, but the accessor looked
//! it up by position: `comments(:one)` was `Comment.find(1)`, "Couldn't
//! find Comment with 'id'=1".

use super::emit_and_run;
use roundhouse::project::BuildTarget;
use std::process::Command;

/// real-blog with its first comment keyed 1001. No fixture refers to a
/// comment, so no label reference has to find it.
fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("test/fixtures/comments.yml", "one:\n  article: one\n", "one:\n  id: 1001\n  article: one\n")
        .write("test/models/fixture_id_test.rb", TEST)
}

const TEST: &str = r#"require "test_helper"

class FixtureIdTest < ActiveSupport::TestCase
  test "an explicit fixture id is the record's id" do
    assert_equal 1001, comments(:one).id
    assert_equal "Alice", comments(:one).commenter
    assert_equal "Bob", comments(:two).commenter
  end
end
"#;

#[test]
fn a_fixture_with_its_own_id_is_found_by_it() {
    app().run_test("test/models/fixture_id_test.rb").assert_passes();
}

/// The same test, compiled with Spinel as the emitted Makefile's
/// spinel-test recipe compiles one, on SQLite.
#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_fixture_with_its_own_id_is_found_by_it_on_spinel() {
    let (emitted, errors) = app().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let compiler = std::env::var("SPINEL").unwrap_or_else(|_| "spinel".into());
    let compiled = Command::new(&compiler)
        .args(["-O", "0", "--no-inline-hot", "test/fixture_id_test.rb", "-o", "fixture_id_test"])
        .current_dir(&*emitted)
        .output()
        .expect("spawn spinel");
    assert!(
        compiled.status.success(),
        "spinel test/fixture_id_test.rb in {}:\n{}{}",
        emitted.display(),
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
    let run = Command::new(emitted.join("fixture_id_test")).current_dir(&*emitted).output().expect("run");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success() && stdout.contains("FixtureIdTest: 1 tests passed"),
        "{stdout}{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

/// A target whose insert leaves an integer key to the database (all but
/// the Ruby family, which writes a supplied key as Rails does) stores
/// the row under the next rowid, not its `id:`. Its accessor keeps
/// finding the row by position, where that rowid is.
#[test]
fn a_target_that_assigns_its_own_keys_finds_the_row_by_position() {
    let (emitted, errors) = app().emit(BuildTarget::Rust);
    assert!(errors.is_empty(), "{errors:#?}");
    let fixtures = std::fs::read_to_string(emitted.join("src/fixtures/comments.rs")).expect("src/fixtures/comments.rs");
    assert!(fixtures.contains("Comment::find(1_i64)") && !fixtures.contains("Comment::find(1001_i64)"), "{fixtures}");
}

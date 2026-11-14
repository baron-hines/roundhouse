//! The Rust target's model surface for an app whose model tests build
//! their own records (`support/rust_record_tests_app.rs`). The first two
//! tests read the emitted tree; the `#[ignore]`d one runs its `cargo
//! test`:
//!
//!     cargo test --test rust_record_tests -- --ignored

use std::process::Command;

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;
#[path = "support/prerequisite.rs"]
mod prerequisite;
#[path = "support/rust_record_tests_app.rs"]
mod rust_record_tests_app;

/// An app whose model tests build their own records compiles on the Rust
/// target: the association reflection's `macro` field is raw-escaped, an
/// abstract `ApplicationRecord` (`self.abstract_class = true`) gets no
/// `to_param` over an `id` it lacks, a model answers `find_by`,
/// `create!`, `valid?`, `destroy!` and `==`, a test helper's omitted
/// keyword defaults are written in at the call, and `setup()` brings up
/// the schema with no fixtures to load.
#[test]
fn record_tests_app_emits_the_model_surface_its_tests_call() {
    let (emitted, errors) = rust_record_tests_app::app().emit(roundhouse::project::BuildTarget::Rust);
    assert!(errors.is_empty(), "{errors:?}");
    let read = |path: &str| std::fs::read_to_string(emitted.join(path)).unwrap_or_else(|_| panic!("{path}"));

    let base = read("src/active_record_base.rs");
    assert!(base.contains("pub r#macro: String,") && !base.contains("pub macro:"), "{base}");

    let record = read("src/models/application_record.rs");
    assert!(!record.contains("fn to_param") && !record.contains("self.id"), "{record}");

    let shelf = read("src/models/shelf.rs");
    for want in ["pub fn find_by<", "pub fn create_bang<", "pub fn valid_pred(&mut self)", "pub fn destroy_bang(&mut self) -> Self", "impl PartialEq for Shelf", "impl PartialEq<Option<Shelf>> for Shelf", "impl PartialEq<Shelf> for Option<Shelf>"] {
        assert!(shelf.contains(want), "missing `{want}`:\n{shelf}");
    }

    let fixtures = read("src/fixtures/mod.rs");
    assert!(fixtures.contains("crate::db::setup_test_db(crate::schema_sql::CREATE_TABLES);"), "{fixtures}");

    let item_test = read("src/tests/item.rs");
    assert!(item_test.contains("make_shelf(1_i64, 2_i64)") && !item_test.contains("Self::make_shelf") && item_test.contains("self::make_shelf("), "{item_test}");
    assert!(item_test.contains(", 7_i64)") && !item_test.contains("Self::padded_size"), "{item_test}");
}

/// `validates :x, presence:` on a NOT NULL integer with no default, and
/// a scoped `uniqueness:`, are lowered on a strict target (the column
/// starts unset; the rule queries the table), so the build reports
/// neither.
#[test]
fn presence_and_uniqueness_are_lowered_on_strict_targets() {
    let warnings: Vec<String> = rust_record_tests_app::app()
        .emit_warnings(roundhouse::project::BuildTarget::Rust)
        .into_iter()
        .map(|d| d.message)
        .filter(|m| m.starts_with("validates :"))
        .collect();
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// The same app's emitted `cargo test`: every test whose rules the target
/// lowers passes (`rust_record_tests_app::PASSING`).
#[test]
#[ignore]
fn record_tests_app_cargo_test_passes_what_it_lowers() {
    let (emitted, errors) = rust_record_tests_app::app().emit(roundhouse::project::BuildTarget::Rust);
    assert!(errors.is_empty(), "{errors:?}");
    let output = Command::new("cargo")
        .args(["test", "--", "--test-threads=1"])
        .current_dir(&*emitted)
        .output()
        .expect("run cargo test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test result:"), "the emitted project did not build:\n{}", String::from_utf8_lossy(&output.stderr));
    for name in rust_record_tests_app::PASSING {
        assert!(stdout.contains(&format!("tests::{name} ... ok")), "{name}:\n{stdout}");
    }
}

/// A constant a test names that nothing defines (here a digest class
/// nothing declares) is a `check` error, as the same
/// constant in the app's own code is. Before, test bodies were not
/// walked for it, and the Rust project's test build failed on an
/// undefined type with nothing reported.
#[test]
fn an_unresolved_constant_in_a_test_is_reported() {
    let (_emitted, errors) = rust_record_tests_app::app()
        .write(
            "test/models/digest_test.rb",
            "require \"test_helper\"\n\nclass DigestTest < ActiveSupport::TestCase\n  test \"a digest\" do\n    assert_nil Shelf.labelled(Digest::NOT_DEFINED_IN_TEST.hexdigest(\"x\"))\n  end\nend\n",
        )
        .emit(roundhouse::project::BuildTarget::Rust);
    assert!(
        errors.iter().any(|e| e.contains("constant not supported") && e.contains("Digest::NOT_DEFINED_IN_TEST")),
        "{errors:?}"
    );
}

/// `assert_respond_to` / `refute_respond_to` with a Symbol are answered
/// at compile time (`lower::respond_to`), from instance methods only: the
/// model's columns, associations and persistence methods answer `true`;
/// another table's column, or a class method (the model's own
/// `self.labelled`, `Base.find`), `false`. The seven assertions of the
/// test, in source order, are three asserts that hold and four refutes
/// that hold.
#[test]
fn respond_to_assertions_fold_on_the_rust_target() {
    let (emitted, errors) = rust_record_tests_app::app().emit(roundhouse::project::BuildTarget::Rust);
    assert!(errors.is_empty(), "{errors:?}");
    let test = std::fs::read_to_string(emitted.join("src/tests/shelf.rs")).expect("shelf test");
    let body = test
        .split("fn test_a_shelf_answers_to_its_columns")
        .nth(1)
        .and_then(|rest| rest.split("#[test]").next())
        .expect("the respond_to test");
    assert!(!body.contains(".respond_to") && !body.contains("respond_to_pred("), "every respond_to? folded:\n{body}");
    let answers: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|line| line.contains("assertion failed") || line.contains("refute failed"))
        .map(|line| line.split(" { panic!").next().unwrap())
        .collect();
    assert_eq!(
        answers,
        ["if !(true)", "if !(true)", "if !(true)", "if false", "if false", "if false", "if false"],
        "{body}"
    );
}

/// What the test above expects, from Rails (the oracle the developer selected
/// with `RH_RAILS_ORACLE`, else the pinned release; an instance's `respond_to?`
/// for a column, association, persistence method or class method does not vary
/// across Rails releases): an instance answers to
/// its column and its association, and not to the class's own method
/// (`def self.labelled`) or to another table's column.
#[test]
fn rails_answers_the_same_respond_to_questions() {
    let oracle = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/rails_oracle.rb");
    let components = r#""activerecord", sqlite3: ">= 2.9""#;
    let probe = emit_and_run::ruby()
        .args(["-e", &format!("require {oracle:?}; puts RailsOracle.available?({components})")])
        .output()
        .expect("oracle availability");
    let available = String::from_utf8_lossy(&probe.stdout).trim().to_owned();
    assert!(
        probe.status.success() && matches!(available.as_str(), "true" | "false"),
        "oracle availability: {available}\n{}",
        String::from_utf8_lossy(&probe.stderr)
    );
    if available == "false" {
        prerequisite::missing("native Rails oracle: missing activerecord, sqlite3");
        return;
    }
    let program = format!(
        r#"require {oracle:?}
RailsOracle.activate({components})
require 'active_record'
ActiveRecord::Base.establish_connection(adapter: 'sqlite3', database: ':memory:')
ActiveRecord::Schema.verbose = false
ActiveRecord::Schema.define do
  create_table(:shelves) {{ |t| t.integer :aisle; t.integer :position; t.string :label }}
  create_table(:items) {{ |t| t.integer :shelf_id; t.string :name; t.integer :weight }}
end
class Item < ActiveRecord::Base; end
class Shelf < ActiveRecord::Base
  has_many :items
  def self.labelled(text) = find_by(label: text)
end
shelf = Shelf.new(aisle: 1, position: 2)
got = %i[aisle items save shelf_id weight labelled find].map {{ |name| shelf.respond_to?(name) }}
raise got.inspect unless got == [true, true, true, false, false, false, false]
"#
    );
    let result = emit_and_run::ruby().args(["-e", &program]).output().expect("native Rails oracle");
    assert!(result.status.success(), "native Rails: {}", String::from_utf8_lossy(&result.stderr));
}

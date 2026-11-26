//! A test class body statement the ingest does not model is ledgered,
//! not dropped silently: `INPUT[:accent] = ACCENT` between two constants
//! changes what a later constant is built from, and the emitted class
//! goes without it (`test_class_statement_between_constants` is the spec
//! that runs it). A bare `private` above helper methods is not reported:
//! the class's own tests call them either way.

use super::emit_and_run;

#[test]
fn a_dropped_test_class_statement_is_ledgered() {
    let warnings = emit_and_run::empty_app()
        .write(
            "test/models/swatch_test.rb",
            r##"require "test_helper"

class SwatchTest < ActiveSupport::TestCase
  ACCENT = { tone: "warm" }
  INPUT = { name: "base" }
  INPUT[:accent] = ACCENT
  EXPECTED = INPUT.dup

  test "the statement ran between the constants" do
    assert_equal({ name: "base", accent: { tone: "warm" } }, EXPECTED)
  end

  private

  def helper
    EXPECTED
  end
end
"##,
        )
        .ingest_warnings();
    let reported: Vec<_> = warnings.iter().filter(|w| w.contains("in the body of test class `SwatchTest`")).collect();
    assert!(
        reported.len() == 1 && reported[0].contains("`INPUT[:accent] = ACCENT`"),
        "{warnings:#?}"
    );
}

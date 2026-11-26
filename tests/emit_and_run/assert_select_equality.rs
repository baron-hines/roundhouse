//! `assert_select`'s equality argument as rails-dom-testing reads it
//! (`HTMLSelector#equality_tests`): `false` or `0` means no element may
//! match, `true` that one must. A matching element under `false` or `0`
//! fails the test, and a `0` substituted into a `?` is a value, not an
//! equality. Run first in the generated blog on Rails 8.1
//! (`bin/rails test`): the first test passes and the three others fail,
//! each with its expectation.

use super::emit_and_run;

const FILE: &str = "test/controllers/listings_controller_test.rb";

const PASSING: &str = r##"require "test_helper"

class ListingsControllerTest < ActionDispatch::IntegrationTest
  test "false and zero assert absence" do
    get articles_url
    assert_response :success
    assert_select "h1", true
    assert_select "h1", "Articles"
    assert_select "aside.sidebar", false
    assert_select "table", 0
    assert_select "p#notice", false
    assert_select "a[href='/articles?page=2']", false
  end
"##;

const FAILING: &str = r##"
  test "false fails on a match" do
    get articles_url
    assert_select "h1", false
  end

  test "zero fails on a match" do
    get articles_url
    assert_select "#articles", 0
  end

  test "a zero substituted into the selector is its value" do
    get articles_url
    assert_select "h1[data-rank=?]", 0
  end
"##;

fn app(test: &str) -> emit_and_run::Overlay {
    emit_and_run::real_blog().write(FILE, test)
}

#[test]
fn assert_select_false_and_zero_assert_absence() {
    let run = app(&format!("{PASSING}{FAILING}end\n")).run_test(FILE);
    assert!(run.errors.is_empty(), "{:?}", run.errors);
    let out = format!("{}{}", run.stdout, run.stderr);
    for failing in [
        "test_false_fails_on_a_match",
        "test_zero_fails_on_a_match",
        "test_a_zero_substituted_into_the_selector_is_its_value",
    ] {
        assert!(out.contains(&format!("FAIL ListingsControllerTest#{failing}")), "{failing} did not fail:\n{out}");
    }
    assert_eq!(out.matches("FAIL ").count(), 3, "{out}");
}

#[test]
#[ignore = "requires spin and spinel on PATH plus SQLite development libraries"]
fn assert_select_false_and_zero_assert_absence_on_spinel() {
    // Only the passing test: a failing one would fail the binary.
    let (emitted, errors) = app(&format!("{PASSING}end\n")).emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    let file = "test/listings_controller_test.rb";
    assert!(emitted.join(file).exists(), "{file} was not emitted in {}", emitted.display());
    let output = std::process::Command::new("spin").args(["test", file]).current_dir(&*emitted).output().expect("spawn spin");
    assert!(
        output.status.success(),
        "spin test failed in {}\n=== stdout ===\n{}\n=== stderr ===\n{}",
        emitted.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

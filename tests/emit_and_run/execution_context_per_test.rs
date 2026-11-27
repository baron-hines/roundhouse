//! Each test runs in a fresh `ActiveSupport::ExecutionContext`, as Rails'
//! executor around the test case gives it (`executor_around_test_case`,
//! on since `load_defaults 7.0`): what one test writes with
//! `Rails.error.set_context` is gone in the next, whichever runs first.
//! Ignored as a fork spec until the compiler does; the test passes
//! unchanged in the generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/models/error_context_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write(
        FILE,
        r##"require "test_helper"

class ErrorContextTest < ActiveSupport::TestCase
  test "the first test's context is its own" do
    assert_nil ActiveSupport::ExecutionContext.to_h[:shelf]
    Rails.error.set_context(lamp: "on")
    assert_equal "on", ActiveSupport::ExecutionContext.to_h[:lamp]
  end

  test "the second test's context is its own" do
    assert_nil ActiveSupport::ExecutionContext.to_h[:lamp]
    Rails.error.set_context(shelf: "full")
    assert_equal "full", ActiveSupport::ExecutionContext.to_h[:shelf]
  end
end
"##,
    )
}

#[test]
#[ignore = "fork spec: test framework"]
fn each_test_runs_in_a_fresh_execution_context() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: test framework"]
fn each_test_runs_in_a_fresh_execution_context_on_spinel() {
    app().run_spin_test("test/error_context_test.rb").assert_passes();
}

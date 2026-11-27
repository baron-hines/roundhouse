//! Minitest calls a test's lifecycle hooks on the test itself, so a
//! private `before_setup`, `after_setup`, `before_teardown` or
//! `after_teardown` the test class overrides (calling `super`) runs around
//! each test: the test sees the setup hooks, and its teardown block runs
//! after `before_teardown` and inside `after_teardown`. Ignored as a fork
//! spec until the compiler does; the test passes unchanged in the
//! generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/models/lifecycle_hooks_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write(
        FILE,
        r##"require "test_helper"

class LifecycleHooksTest < ActiveSupport::TestCase
  teardown do
    assert_equal [:before_setup, :after_setup, :before_teardown, :after_teardown], @hooks
  end

  test "each private hook runs" do
    assert_equal [:before_setup, :after_setup], @hooks
  end

  private
    def before_setup
      @hooks = [:before_setup]
      super
    end

    def after_setup
      super
      @hooks << :after_setup
    end

    def before_teardown
      super
      @hooks << :before_teardown
    end

    def after_teardown
      @hooks << :after_teardown
      super
    end
end
"##,
    )
}

#[test]
#[ignore = "fork spec: test framework"]
fn the_runner_calls_private_lifecycle_hooks() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: test framework"]
fn the_runner_calls_private_lifecycle_hooks_on_spinel() {
    app().run_spin_test("test/lifecycle_hooks_test.rb").assert_passes();
}

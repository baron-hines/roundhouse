//! A module in a test folder (`test_paths: [test/support]`) that a test
//! class includes. Ingest read every file there as a test file, so a
//! file holding only `module ArgsHelper` produced nothing: the including
//! test passed `check` and died `uninitialized constant
//! ArgsTest::ArgsHelper` when run. The module is now carried into each
//! including test's file, as a `test/test_helpers/` module is, and the
//! instance methods the test reaches are spliced in as its helpers, so
//! their assertions are lowered like the test's own. Its `test_*`
//! methods are the including class's tests, as Minitest runs them.
//! `flunk` lowers to a raise of its message. (Cases from
//! Shopify/roundhouse-1's census.)
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

const HELPER: &str = r#"module ArgsHelper
  def assert_args_present
    flunk("@args instance variable is not defined") unless defined?(@args)

    assert_equal({ id: 1 }, @args)
  end

  def label
    "helper"
  end

  def test_from_the_module
    assert_equal "test", label
  end
end
"#;

const TEST: &str = r#"require "test_helper"
require "support/args_helper"

class ArgsTest < ActiveSupport::TestCase
  include ArgsHelper

  def label
    "test"
  end

  test "a writer in the test body satisfies the helper" do
    @args = { id: 1 }
    assert_args_present
  end

  test "the class's own method beats the module's" do
    assert_equal "test", label
  end
end
"#;

const FLUNKING: &str = r#"require "test_helper"
require "support/args_helper"

class FlunkTest < ActiveSupport::TestCase
  include ArgsHelper

  test "the helper's guard flunks with its message" do
    assert_args_present
  end

  test "flunk in a test body" do
    flunk "stop here"
  end

  test "flunk without a message" do
    flunk
  end
end
"#;

// Asserts nothing, so it is carried and not spliced: the module's own
// methods answer. The test-method lowering a splice would put
// `boxed` through drops the `**` of `Box.new(**options)` (campfire's
// `with_push_service`, `Server.new(**options)`).
const FORWARDING_HELPER: &str = r#"module ForwardingHelper
  class Box
    attr_reader :name

    def initialize(name:)
      @name = name
    end
  end

  def boxed(**options)
    Box.new(**options).name
  end
end
"#;

const FORWARDING_TEST: &str = r#"require "test_helper"

class ForwardingTest < ActiveSupport::TestCase
  include ForwardingHelper

  test "a forwarded keyword splat reaches the nested class" do
    assert_equal "x", boxed(name: "x")
  end
end
"#;

// A carried module's method named like a route helper is the class's
// own, as `include` makes it: not `RouteHelpers.generate_file_path`.
const PATH_HELPER: &str = r#"module PathHelper
  def generate_file_path(name)
    "files/#{name}"
  end
end
"#;

const PATH_TEST: &str = r#"require "test_helper"

class PathTest < ActiveSupport::TestCase
  include PathHelper

  test "the module's _path method answers, not a route" do
    assert_equal "files/a", generate_file_path("a")
  end
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("roundhouse.yml", "test_paths:\n  - test/support\n")
        .write("test/support/args_helper.rb", HELPER)
        .write("test/models/args_test.rb", TEST)
        .write("test/models/flunk_test.rb", FLUNKING)
        .write("test/support/forwarding_helper.rb", FORWARDING_HELPER)
        .write("test/models/forwarding_test.rb", FORWARDING_TEST)
        .write("test/support/path_helper.rb", PATH_HELPER)
        .write("test/models/path_test.rb", PATH_TEST)
}

#[test]
fn an_included_test_folder_module_runs_with_lowered_assertions() {
    let run = app().run_test("test/models/args_test.rb");
    run.assert_passes();
    // Two of the class's own and the module's `test_from_the_module`.
    assert!(run.stdout.contains("ArgsTest: 3 tests passed"), "{}", run.stdout);
}

#[test]
fn flunk_fails_the_test_with_its_message() {
    let run = app().run_test("test/models/flunk_test.rb");
    for line in [
        "FAIL FlunkTest#test_the_helper_s_guard_flunks_with_its_message: @args instance variable is not defined",
        "FAIL FlunkTest#test_flunk_in_a_test_body: stop here",
        "FAIL FlunkTest#test_flunk_without_a_message: Epic Fail!",
    ] {
        assert!(run.stdout.contains(line), "missing {line:?}:\n{}", run.stdout);
    }
}

#[test]
fn a_module_that_asserts_nothing_answers_through_the_include() {
    let run = app().run_test("test/models/forwarding_test.rb");
    run.assert_passes();
    assert!(run.stdout.contains("ForwardingTest: 1 tests passed"), "{}", run.stdout);
}

#[test]
fn a_carried_modules_path_named_method_is_not_a_route_helper() {
    let run = app().run_test("test/models/path_test.rb");
    run.assert_passes();
    assert!(run.stdout.contains("PathTest: 1 tests passed"), "{}", run.stdout);
}

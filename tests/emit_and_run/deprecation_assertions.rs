//! ActiveSupport's deprecation assertions exist on the test base, as in
//! Rails: `assert_deprecated` answers the block's value and checks a
//! deprecator warned, matching its message, `assert_not_deprecated`
//! checks none did, and `collect_deprecations` answers the block's value
//! and the messages. Ignored as a fork spec until the compiler does; the
//! test passes unchanged in the generated blog on Rails 8.1.4
//! (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/models/shelf_deprecation_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "app/models/shelf_notes.rb",
            r##"class ShelfNotes
  def self.deprecator
    @deprecator ||= ActiveSupport::Deprecation.new("2.0", "Shelf")
  end

  def self.old_count
    deprecator.warn("old_count is going away")
    3
  end

  def self.count
    3
  end
end
"##,
        )
        .write(
            FILE,
            r##"require "test_helper"

class ShelfDeprecationTest < ActiveSupport::TestCase
  test "the deprecation assertions" do
    assert_equal 3, assert_deprecated(/going away/, ShelfNotes.deprecator) { ShelfNotes.old_count }
    assert_equal 3, assert_not_deprecated(ShelfNotes.deprecator) { ShelfNotes.count }
    value, messages = collect_deprecations(ShelfNotes.deprecator) { ShelfNotes.old_count }
    assert_equal 3, value
    assert_equal 1, messages.size
    assert_match(/old_count is going away/, messages.first)
  end
end
"##,
        )
}

#[test]
#[ignore = "fork spec: test framework"]
fn the_deprecation_assertions_exist_on_the_test_base() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: test framework"]
fn the_deprecation_assertions_exist_on_the_test_base_on_spinel() {
    app().run_spin_test("test/shelf_deprecation_test.rb").assert_passes();
}

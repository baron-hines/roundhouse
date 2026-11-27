//! A test class that includes a Concern whose `included` block runs a
//! statement that declares tests lists those tests as it loads, as
//! Minitest collects them: the helper's `in_both_modes` defines a variant
//! of each test the class declared before the include. Ignored as a fork
//! spec until the compiler does; the test passes unchanged in the
//! generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/models/lamp_modes_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "test/test_helper.rb",
            "require \"rails/test_help\"\n",
            "require \"rails/test_help\"\nrequire_relative \"test_helpers/lamp_modes\"\n",
        )
        .write(
            "test/test_helpers/lamp_modes.rb",
            r##"module BothModes
  extend ActiveSupport::Concern

  class_methods do
    def in_both_modes
      instance_methods(false).grep(/\Atest_/).each do |name|
        define_method("#{name}_dimmed") do
          @dimmed = true
          send(name)
        end
      end
    end
  end
end

module LampModes
  extend ActiveSupport::Concern

  included do
    in_both_modes
  end
end
"##,
        )
        .write(
            FILE,
            r##"require "test_helper"

class LampModesTest < ActiveSupport::TestCase
  include BothModes

  test "the lamp shines" do
    assert_includes [nil, true], @dimmed
  end

  include LampModes

  test "the variant is listed" do
    assert_includes self.class.runnable_methods, "test_the_lamp_shines_dimmed"
  end
end
"##,
        )
}

#[test]
#[ignore = "fork spec: test framework"]
fn a_concerns_included_block_lists_the_tests_it_declares() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: test framework"]
fn a_concerns_included_block_lists_the_tests_it_declares_on_spinel() {
    app().run_spin_test("test/lamp_modes_test.rb").assert_passes();
}

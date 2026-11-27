//! A setup block a support module's `included` block registers runs as a
//! block of that module, as ActiveSupport runs it with `instance_exec`:
//! its `self` is the test, it runs before the class's own setup, and its
//! frame is the block written in the module (`caller_locations` names the
//! helper's file and the module's body), not a method of the test file.
//! The first two already hold on `main`, which copies the block's body
//! into the test; the frame is what this waits on. Ignored as a fork spec
//! until the compiler does; the test passes unchanged in the generated
//! blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/models/lamp_frames_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "test/test_helper.rb",
            "require \"rails/test_help\"\n",
            "require \"rails/test_help\"\nrequire_relative \"test_helpers/lamp_frames\"\n",
        )
        .write(
            "test/test_helpers/lamp_frames.rb",
            r##"module LampFrames
  extend ActiveSupport::Concern

  included do
    setup do
      frame = caller_locations(0, 1).first
      @frames = [[File.basename(frame.path), frame.base_label, self.class.name]]
    end
  end
end
"##,
        )
        .write(
            FILE,
            r##"require "test_helper"

class LampFramesTest < ActiveSupport::TestCase
  include LampFrames

  setup do
    @frames << :own
  end

  test "the module's setup ran as its own block" do
    assert_equal [["lamp_frames.rb", "<module:LampFrames>", "LampFramesTest"], :own], @frames
  end
end
"##,
        )
}

#[test]
#[ignore = "fork spec: test framework"]
fn an_included_setup_block_runs_as_a_block_of_its_module() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: test framework"]
fn an_included_setup_block_runs_as_a_block_of_its_module_on_spinel() {
    app().run_spin_test("test/lamp_frames_test.rb").assert_passes();
}

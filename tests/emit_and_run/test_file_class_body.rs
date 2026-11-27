//! A class written in a test file runs its body's calls as the file
//! loads, as Ruby does: a registry class beside the test class calls its
//! own class method in its body (`stock :birch`), and the test sees what
//! the call recorded. The test passes unchanged in the generated blog on
//! Rails 8.1.4 (`bin/rails test`).
//!
//! The same class shared by two test classes of one file is still
//! defined once per test file the emit writes: `test_file_shared_class_once`.

use super::emit_and_run;

const FILE: &str = "test/models/timber_yard_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write(
        FILE,
        r##"require "test_helper"

class TimberYard
  def self.stocked
    @stocked ||= []
  end

  def self.stock(name, count = 1)
    stocked << [name, count]
  end

  stock :birch
  stock :ash, 3
end

class TimberYardTest < ActiveSupport::TestCase
  test "the yard's body ran its calls" do
    assert_equal [[:birch, 1], [:ash, 3]], TimberYard.stocked
  end
end
"##,
    )
}

#[test]
fn a_test_files_class_runs_its_body_calls() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_test_files_class_runs_its_body_calls_on_spinel() {
    app().run_spin_test("test/timber_yard_test.rb").assert_passes();
}

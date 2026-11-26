//! A test class's body runs in the order it is written, as Ruby runs any
//! class body: a statement between two constants changes the first before
//! the second is built from it. Ignored as a fork spec until the compiler
//! does; the test passes unchanged in the generated blog on Rails 8.1.4
//! (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/models/palette_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write(
        FILE,
        r##"require "test_helper"

class PaletteTest < ActiveSupport::TestCase
  ACCENT = { shade: "dark" }
  INPUT = { name: "base" }
  INPUT[:accent] = ACCENT
  EXPECTED = INPUT.stringify_keys
  EXPECTED["accent"] = EXPECTED["accent"].stringify_keys

  test "the statements ran between the constants" do
    assert_equal({ "name" => "base", "accent" => { "shade" => "dark" } }, EXPECTED)
  end
end
"##,
    )
}

#[test]
#[ignore = "fork spec: test framework"]
fn a_test_class_body_runs_its_statements_in_order() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: test framework"]
fn a_test_class_body_runs_its_statements_in_order_on_spinel() {
    app().run_spin_test("test/palette_test.rb").assert_passes();
}

//! An include of ActionView's TranslationHelper is kept wherever it is
//! written, so a test case that includes it calls `t` and `translate`
//! from its test bodies, as Rails' does: the key is read from the app's
//! locale files, with its interpolation. Ignored as a fork
//! spec until the compiler does; the test passes unchanged in the
//! generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/models/lantern_copy_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "config/locales/lanterns.en.yml",
            r#"en:
  lanterns:
    title: "Paper lanterns"
    count: "%{count} lit"
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class LanternCopyTest < ActiveSupport::TestCase
  include ActionView::Helpers::TranslationHelper

  test "t translates from a test body" do
    assert_equal "Paper lanterns", t("lanterns.title")
    assert_equal "3 lit", translate("lanterns.count", count: 3)
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: views helpers"]
fn a_test_case_including_translation_helper_translates() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: views helpers"]
fn a_test_case_including_translation_helper_translates_on_spinel() {
    app().run_spin_test("test/lantern_copy_test.rb").assert_passes();
}

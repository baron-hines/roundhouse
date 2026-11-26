//! A module whose singleton includes ActionView's TagHelper
//! (`class << self; include ActionView::Helpers::TagHelper; end`) answers
//! `content_tag`, `tag` and `safe_join` for any caller, as Action View's
//! do: content that is not html_safe is escaped, html_safe content is
//! kept, and the element is html_safe. Ignored as a fork spec until the
//! compiler does; the test passes unchanged in the generated blog on
//! Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/models/plaque_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "app/models/markup.rb",
            r#"module Markup
  class << self
    include ActionView::Helpers::TagHelper
  end
end
"#,
        )
        .write(
            "app/models/plaque.rb",
            r#"class Plaque
  def initialize(lines)
    @lines = lines
  end

  def to_html
    Markup.content_tag(:p, Markup.safe_join(@lines, Markup.tag(:br)), class: "plaque")
  end
end
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class PlaqueTest < ActiveSupport::TestCase
  test "a singleton TagHelper include answers any caller" do
    assert_equal "<p>a &lt; b</p>", Markup.content_tag(:p, "a < b")
    assert_equal "<p><em>kept</em></p>", Markup.content_tag(:p, "<em>kept</em>".html_safe)
    assert Markup.content_tag(:span, "x").html_safe?
    assert_equal "<br />", Markup.tag(:br)
    assert_equal "<input type=\"text\" />", Markup.tag(:input, type: "text")
    assert_equal "a &amp; b, <b>c</b>", Markup.safe_join(["a & b", "<b>c</b>".html_safe], ", ")
    assert_equal "<p class=\"plaque\">Oak &amp; Ash<br />1921</p>", Plaque.new(["Oak & Ash", "1921"]).to_html
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: views helpers"]
fn a_singleton_tag_helper_include_answers_any_caller() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: views helpers"]
fn a_singleton_tag_helper_include_answers_any_caller_on_spinel() {
    app().run_spin_test("test/plaque_test.rb").assert_passes();
}

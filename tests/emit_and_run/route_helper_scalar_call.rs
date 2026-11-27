//! In a test body, a route helper's positional that is a method call
//! answering a String or an Integer fills its segment as written, as
//! Rails' `to_param` does: no `.id` is called on it. The test passes
//! unchanged in the generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/models/article_slugs_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "app/models/article_slugs.rb",
            r##"class ArticleSlugs
  def self.code(n)
    "a#{n}"
  end

  def self.width(label)
    label.to_s.length * 2
  end
end
"##,
        )
        .write(
            FILE,
            r##"require "test_helper"

class ArticleSlugsTest < ActionDispatch::IntegrationTest
  test "a call answering a scalar fills a segment" do
    assert_equal "/articles/a7", article_path(ArticleSlugs.code(7))
    assert_equal "/articles/10", article_path(ArticleSlugs.width("hello"))
    assert_equal "/articles/a1/comments/a2", article_comment_path(ArticleSlugs.code(1), ArticleSlugs.code(2))
  end
end
"##,
        )
}

#[test]
fn a_route_helper_takes_a_scalar_call_as_written() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_route_helper_takes_a_scalar_call_as_written_on_spinel() {
    app().run_spin_test("test/article_slugs_test.rb").assert_passes();
}

//! `assert_difference` answers its block's value, as ActiveSupport's
//! does, so a test binds the record it made, or destructures an Array
//! the block answers. Ignored as a fork spec until the compiler does; the
//! test passes unchanged in the generated blog on Rails 8.1.4
//! (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/models/article_made_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write(
        FILE,
        r##"require "test_helper"

class ArticleMadeTest < ActiveSupport::TestCase
  test "the block's value comes back" do
    article = assert_difference("Article.count") do
      Article.create!(title: "Made", body: "A body long enough to save.")
    end
    assert_equal "Made", article.title
    count, title = assert_difference("Article.count", 1) do
      made = Article.create!(title: "Again", body: "Another body long enough.")
      [Article.count, made.title]
    end
    assert_equal 4, count
    assert_equal "Again", title
  end
end
"##,
    )
}

#[test]
#[ignore = "fork spec: test framework"]
fn assert_difference_answers_its_blocks_value() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: test framework"]
fn assert_difference_answers_its_blocks_value_on_spinel() {
    app().run_spin_test("test/article_made_test.rb").assert_passes();
}

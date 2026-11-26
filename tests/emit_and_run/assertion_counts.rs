//! A test counts its assertions as Minitest and ActiveSupport do, and
//! reads the count with `assertions`: `assert_equal` and `assert_raises`
//! count one each, `assert_difference` one per expression it watches and
//! one for the block raising nothing, `assert_no_difference` two, and
//! `assert_changes` with `from:` and `to:` four. Ignored as a fork spec
//! until the compiler does; the test passes unchanged in the generated
//! blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/models/article_tally_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write(
        FILE,
        r##"require "test_helper"

class ArticleTallyTest < ActiveSupport::TestCase
  test "each assertion adds what Rails counts" do
    assert_equal 2, Article.count
    assert_equal 1, assertions
    assert_raises(ArgumentError) { raise ArgumentError }
    assert_equal 3, assertions
    assert_difference ["Article.count", "Comment.count"] do
      Article.create!(title: "Tallied", body: "A body long enough to save.").comments.create!(commenter: "Ann", body: "Nice")
    end
    assert_equal 7, assertions
    assert_no_difference("Article.count") { Article.first.title }
    assert_equal 10, assertions
    article = Article.first
    assert_changes -> { article.title }, from: article.title, to: "Retitled" do
      article.title = "Retitled"
    end
    assert_equal 15, assertions
  end
end
"##,
    )
}

#[test]
#[ignore = "fork spec: test framework"]
fn a_test_counts_its_assertions_as_rails_does() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: test framework"]
fn a_test_counts_its_assertions_as_rails_does_on_spinel() {
    app().run_spin_test("test/article_tally_test.rb").assert_passes();
}

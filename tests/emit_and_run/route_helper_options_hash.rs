//! A route helper given an options Hash past its segments reads it as
//! `url_for`'s options, as Rails' helper does: `params:` and the keys no
//! segment takes are the sorted query, and `anchor:` is the fragment after
//! it. `article_path(article, options)` with the Hash in a local answers
//! `/articles/<id>?page=2&sort=new#comments`. Ignored as a fork spec until
//! the compiler does; the test passes unchanged in the generated blog on
//! Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/models/article_links_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "app/models/article_links.rb",
            r##"class ArticleLinks
  def self.link(article, options)
    Rails.application.routes.url_helpers.article_path(article, options)
  end
end
"##,
        )
        .write(
            FILE,
            r##"require "test_helper"

class ArticleLinksTest < ActiveSupport::TestCase
  test "a trailing Hash is url_for's options" do
    article = articles(:one)
    options = { params: { page: 2 }, anchor: "comments", sort: "new" }
    assert_equal "/articles/#{article.id}?page=2&sort=new#comments", ArticleLinks.link(article, options)
    assert_equal "/articles/#{article.id}?page=3", ArticleLinks.link(article, { params: { page: 3 } })
  end
end
"##,
        )
}

#[test]
#[ignore = "fork spec: routes"]
fn a_route_helpers_trailing_hash_is_url_fors_options() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: routes"]
fn a_route_helpers_trailing_hash_is_url_fors_options_on_spinel() {
    app().run_spin_test("test/article_links_test.rb").assert_passes();
}

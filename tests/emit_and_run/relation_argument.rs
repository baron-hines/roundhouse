//! maybe's `EntrySearch.apply_search_filter(scope, search)` and `Transaction::Search`'s `apply_*_filter(query, …)`:
//! a method that calls a query method on a parameter takes the relation Rails' CollectionProxy is, so a has_many
//! read handed to it, directly or from a local, is the association's relation rather than the loaded Array (which
//! answers no `where`). `comments.all` is that relation too.

use super::emit_and_run;

const SEARCH: &str = "class CommentSearch\n  class << self\n    def apply_filter(scope, q)\n      return scope if q.blank?\n\n      query = scope\n      query = query.where(commenter: q)\n      query\n    end\n  end\nend\n";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/comment_search.rb", SEARCH).edit(
        "app/models/article.rb",
        "class Article < ApplicationRecord\n",
        "class Article < ApplicationRecord\n  def first_matching(q)\n    CommentSearch.apply_filter(comments, q).order(:id).first&.commenter\n  end\n\n  def through_local(q)\n    found = comments\n    found = CommentSearch.apply_filter(found, q)\n    found.order(:id).map(&:commenter).join(\",\")\n  end\n\n  def every_comment = comments.all.to_a.size\n\n",
    )
}

const ASSERTIONS: &str = r##"
a = Article.create!(title: "Hello", body: "A body long enough for validation.")
b = Article.create!(title: "Other", body: "A body long enough for validation.")
Comment.create!(article: a, commenter: "Ann", body: "x")
Comment.create!(article: a, commenter: "Bob", body: "y")
Comment.create!(article: b, commenter: "Bob", body: "z")
a = Article.find(a.id)
raise a.first_matching("Bob").inspect unless a.first_matching("Bob") == "Bob"
raise a.first_matching("").inspect unless a.first_matching("") == "Ann"
raise a.through_local("Bob").inspect unless a.through_local("Bob") == "Bob"
raise a.through_local(nil).inspect unless a.through_local(nil) == "Ann,Bob"
raise a.every_comment.inspect unless a.every_comment == 2
puts "relation argument passed"
"##;

#[test]
fn a_has_many_read_handed_to_a_relation_parameter_is_its_relation() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("relation argument passed"), "{}", run.stdout);
}

/// A local holding records the pass cannot seed (`to_a`) handed to the same parameter answers no `where` in Rails
/// either: refused, not emitted to raise.
#[test]
fn loaded_records_handed_to_a_relation_parameter_are_refused() {
    let errors = emit_and_run::real_blog()
        .write("app/models/comment_search.rb", SEARCH)
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  def loaded_matching(q)\n    loaded = comments.to_a\n    CommentSearch.apply_filter(loaded, q)\n  end\n\n",
        )
        .check_errors();
    assert!(errors.iter().any(|e| e.contains("records passed as a relation")), "{errors:#?}");
}

//! A route helper's argument that is a column is passed as it is, even
//! when another model has a `belongs_to` of the same name. lobsters'
//! /tags writes `edit_mod_tag_path(tag.tag)` (Tag's String column, while
//! Moderation `belongs_to :tag`): the argument was taken for a Tag
//! record and projected through `.to_param`, which a Spinel String does
//! not answer, so /tags was a 500 on the Spinel tree. A record read
//! through the receiver's own association keeps its projection.

use super::emit_and_run;
use roundhouse::project::BuildTarget;

#[test]
fn a_column_named_like_another_models_association_is_not_a_record() {
    let (emitted, errors) = emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "    t.integer \"article_id\", null: false\n",
            "    t.integer \"article_id\", null: false\n    t.integer \"body_id\"\n",
        )
        .edit(
            "app/models/comment.rb",
            "class Comment < ApplicationRecord\n",
            "class Comment < ApplicationRecord\n  belongs_to :body, class_name: \"Article\", optional: true\n",
        )
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/labels/:name/edit\" => \"articles#index\", as: \"edit_label\"\n",
        )
        .edit(
            "app/views/articles/_article.html.erb",
            "    <p class=\"text-gray-700 mt-2\">",
            "    <%= link_to \"label\", edit_label_path(article.body), id: \"label\" %>\n    <p class=\"text-gray-700 mt-2\">",
        )
        .emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    let view = std::fs::read_to_string(emitted.join("app/views/articles/_article.rb")).expect("partial");
    assert!(view.contains("edit_label_path(article.body)"), "{view}");
    assert!(!view.contains("article.body.to_param"), "{view}");
}

//! A scope whose body ends in `group_by` / `index_by` over its relation answers the Hash Rails' `scope` hands back
//! (`_exec_scope` returns the body's value), not an Array of records.

use super::emit_and_run;

#[test]
fn a_scope_ending_in_group_by_answers_a_hash() {
    let overlay = || {
        emit_and_run::real_blog()
            .edit(
                "app/models/article.rb",
                "  validates :title, presence: true\n",
                "  validates :title, presence: true\n  scope :by_title, -> { where.not(title: nil).group_by(&:title) }\n  scope :indexed, -> { order(:id).index_by(&:title) }\n\n  def self.title_counts\n    Article.by_title.transform_values { |rows| rows.size }\n  end\n\n  def self.indexed_ids\n    Article.indexed.transform_values(&:id)\n  end\n",
            )
    };
    let errors = overlay().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
    overlay()
        .run_ruby("a = Article.create!(title: \"Hello\", body: \"A long body\")\nArticle.create!(title: \"Hello\", body: \"Another body\")\nArticle.create!(title: \"Bye\", body: \"A third body\")\nraise Article.title_counts.inspect unless Article.title_counts == { \"Hello\" => 2, \"Bye\" => 1 }\nraise Article.indexed_ids.inspect unless Article.indexed_ids[\"Bye\"] == Article.last.id\nputs \"scope hash passed\"\n")
        .assert_passes();
}

//! A `has_many :through` whose source is a polymorphic `belongs_to`
//! named by `source_type:` reads and writes the join rows of that type
//! only, and a presence validation on a `has_many` asks its reader.
//!
//! lobsters' ModMail `has_many :comment_references, through:
//! :mod_mail_references, source: :reference, source_type: "Comment"` (and
//! `:story_references` beside it) had no writer, so creating a mod mail
//! was a NoMethodError, and its reader fell back to a direct has_many
//! over a `comments.mod_mail_id` column that does not exist. Its
//! `validates :recipients, presence: true` read an `@recipients` ivar
//! nothing writes, so every mod mail was invalid.

use super::emit_and_run;

const ASSERTIONS: &str = r#"
a = Article.create!(title: "Pinned", body: "A body long enough for validation.")
c = Comment.create!(article: a, commenter: "x", body: "y")
b = Board.new(name: "b")
raise "blank board saved" if b.save
raise "no blank error: #{b.errors.inspect}" unless b.errors.include?("Pinned articles can't be blank")
b.pinned_articles = [a]
b.pinned_comments = [c]
b.save!
b2 = Board.find(b.id)
raise "articles: #{b2.pinned_articles.map(&:id).inspect}" unless b2.pinned_articles.map(&:id) == [a.id]
raise "comments: #{b2.pinned_comments.map(&:id).inspect}" unless b2.pinned_comments.map(&:id) == [c.id]
b2.pinned_comments = []
b2.save!
b3 = Board.find(b.id)
raise "kept articles: #{b3.pinned_articles.to_a.size}" unless b3.pinned_articles.to_a.size == 1
raise "cleared comments: #{b3.pinned_comments.to_a.size}" unless b3.pinned_comments.to_a.empty?
puts "polymorphic through passed"
"#;

#[test]
fn a_polymorphic_source_through_reads_and_writes_its_own_type() {
    let run = emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "  add_foreign_key \"comments\", \"articles\"\n",
            concat!(
                "  create_table \"boards\", force: :cascade do |t|\n    t.string \"name\"\n    t.datetime \"created_at\", null: false\n    t.datetime \"updated_at\", null: false\n  end\n\n",
                "  create_table \"pins\", force: :cascade do |t|\n    t.integer \"board_id\", null: false\n    t.string \"pinned_type\"\n    t.integer \"pinned_id\"\n    t.datetime \"created_at\", null: false\n    t.datetime \"updated_at\", null: false\n  end\n\n",
                "  add_foreign_key \"comments\", \"articles\"\n",
            ),
        )
        .write(
            "app/models/board.rb",
            concat!(
                "class Board < ApplicationRecord\n",
                "  has_many :pins\n",
                "  has_many :pinned_comments, through: :pins, source: :pinned, source_type: \"Comment\"\n",
                "  has_many :pinned_articles, through: :pins, source: :pinned, source_type: \"Article\"\n\n",
                "  validates :pinned_articles, presence: true\n",
                "end\n",
            ),
        )
        .write(
            "app/models/pin.rb",
            "class Pin < ApplicationRecord\n  belongs_to :board\n  belongs_to :pinned, polymorphic: true\nend\n",
        )
        .run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("polymorphic through passed"), "{}", run.stdout);
}

//! `has_many :memos, class_name: "::Note"` in `Article` beside an
//! `Article::Note` model: the rooted name is the top-level `Note`, which a
//! bare `Note` written inside `class Article` would not reach.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "  create_table \"comments\", force: :cascade do |t|",
            "  create_table \"article_notes\", force: :cascade do |t|\n    t.integer \"article_id\", null: false\n    t.string \"body\"\n  end\n\n  create_table \"notes\", force: :cascade do |t|\n    t.integer \"article_id\", null: false\n    t.string \"body\"\n  end\n\n  create_table \"comments\", force: :cascade do |t|",
        )
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy\n",
            "  has_many :comments, dependent: :destroy\n  has_many :notes, dependent: :destroy\n  has_many :memos, class_name: \"::Note\", dependent: :destroy\n\n  def kinds = [notes.map(&:kind), memos.map(&:kind)]\n",
        )
        .write("app/models/article/note.rb", "class Article::Note < ApplicationRecord\n  belongs_to :article\n\n  def kind = \"nested\"\nend\n")
        .write("app/models/note.rb", "class Note < ApplicationRecord\n  belongs_to :article\n\n  def kind = \"top\"\nend\n")
}

const ASSERTIONS: &str = r#"
a = Article.create!(title: "Hello", body: "A body long enough for validation.")
Article::Note.create!(article: a, body: "n")
Note.create!(article: a, body: "m")
got = Article.find(a.id).kinds
raise got.inspect unless got == [["nested"], ["top"]]
puts "rooted class name passed"
"#;

#[test]
fn a_rooted_class_name_reads_the_top_level_model() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("rooted class name passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_rooted_class_name_reads_the_top_level_model_on_spinel() {
    let run = app().run_spinel_with_db(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("rooted class name passed"), "{}", run.stdout);
}

//! maybe's `Rule`: `has_many :conditions` beside a `Rule::Condition` model.
//! Rails' `compute_type` tries the model's own name first
//! (`"Rule".scan(/::|$/)` gives `Rule::Condition`), so the association
//! reads `Rule::Condition`, not a top-level `Condition`.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "  create_table \"comments\", force: :cascade do |t|",
            "  create_table \"article_notes\", force: :cascade do |t|\n    t.integer \"article_id\", null: false\n    t.string \"body\"\n  end\n\n  create_table \"comments\", force: :cascade do |t|",
        )
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy\n",
            "  has_many :comments, dependent: :destroy\n  has_many :notes, dependent: :destroy\n\n  def note_bodies = notes.map(&:shout)\n",
        )
        .write(
            "app/models/article/note.rb",
            "class Article::Note < ApplicationRecord\n  belongs_to :article\n\n  def shout = body.upcase\nend\n",
        )
}

const ASSERTIONS: &str = r#"
a = Article.create!(title: "Hello", body: "A body long enough for validation.")
Article::Note.create!(article: a, body: "first")
Article::Note.create!(article: a, body: "second")
got = Article.find(a.id).note_bodies.sort
raise got.inspect unless got == ["FIRST", "SECOND"]
raise "class #{a.notes.first.class}" unless a.notes.first.is_a?(Article::Note)
raise "owner" unless Article::Note.first.article.id == a.id
puts "own namespace association passed"
"#;

#[test]
fn an_association_reads_the_models_own_nested_class() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("own namespace association passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_association_reads_the_models_own_nested_class_on_spinel() {
    let run = app().run_spinel_with_db(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("own namespace association passed"), "{}", run.stdout);
}

/// The rules around it, each as Rails 8.1.4 reads them: a nested model
/// under a parent declaring
/// `self.table_name` prefixes that table (`Label::Tag` under
/// `self.table_name = "tags"` reads `tag_tags`), and one a level further
/// takes its parent's effective table (`article_note_marks`); and a class nested in a
/// module (`Billing::Invoice`) keeps its bare table even when a
/// `billings` and a `billing_invoices` table exist.
fn rules_app() -> emit_and_run::Overlay {
    app()
        .edit(
            "db/schema.rb",
            "  create_table \"comments\", force: :cascade do |t|",
            "  create_table \"article_note_marks\", force: :cascade do |t|\n    t.string \"sign\"\n  end\n\n  create_table \"tags\", force: :cascade do |t|\n    t.string \"name\"\n  end\n\n  create_table \"tag_tags\", force: :cascade do |t|\n    t.integer \"label_id\"\n    t.string \"word\"\n  end\n\n  create_table \"billings\", force: :cascade do |t|\n    t.string \"x\"\n  end\n\n  create_table \"billing_invoices\", force: :cascade do |t|\n    t.string \"wrong\"\n  end\n\n  create_table \"invoices\", force: :cascade do |t|\n    t.string \"number\"\n  end\n\n  create_table \"comments\", force: :cascade do |t|",
        )
        .write("app/models/label.rb", "class Label < ApplicationRecord\n  self.table_name = \"tags\"\nend\n")
        .write("app/models/label/tag.rb", "class Label::Tag < ApplicationRecord\n  def loud = word.upcase\nend\n")
        .write("app/models/article/note/mark.rb", "class Article::Note::Mark < ApplicationRecord\n  def double = sign * 2\nend\n")
        .write("app/models/billing.rb", "module Billing\nend\n")
        .write("app/models/billing/invoice.rb", "class Billing::Invoice < ApplicationRecord\n  def label = number.upcase\nend\n")
}

const RULES: &str = r#"
raise "declared parent table" unless Label::Tag.create!(word: "ok").loud == "OK"
raise "two levels" unless Article::Note::Mark.create!(sign: "x").double == "xx"
raise "module parent" unless Billing::Invoice.create!(number: "inv-1").label == "INV-1"
puts "compute_type rules passed"
"#;

#[test]
fn compute_types_declared_and_module_parent_rules_hold() {
    let run = rules_app().run_ruby(RULES);
    run.assert_passes();
    assert!(run.stdout.contains("compute_type rules passed"), "{}", run.stdout);
}

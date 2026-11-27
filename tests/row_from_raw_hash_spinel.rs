//! `<Model>Row.from_raw(row)` keeps its `Hash[String, untyped]` parameter
//! in the Spinel build. Every Hash-keyed hydration (a Relation with an
//! explicit `select`, a `has_many :through` preload batch) used to take
//! the adapter's `select_rows` Array and hand each element to
//! `instantiate`. Spinel can only type an Array of Hashes as a
//! polymorphic array, so the element arrived boxed and the `from_raw`
//! parameter widened to untyped: every attribute read in it went through
//! the boxed dispatch path. The adapter's `each_row` yields each row
//! Hash instead, so the parameter stays a Hash pointer.
//!
//! Needs a Spinel compiler (`$SPINEL`, default `spinel`), so it is
//! `#[ignore]`d like its sibling suites and listed in `scripts/ci-plan.py`'s
//! `SPINEL_TESTS`.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use std::process::Command;

use roundhouse::project::BuildTarget;

fn tagged_articles() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n",
        )
        .write(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n  has_many :taggings\n  has_many :tags, through: :taggings\n\n  def self.tagged\n    Article.includes(:tags).to_a\n  end\n\n  def self.titles\n    Article.select(:id, :title).to_a\n  end\nend\n",
        )
        .write(
            "app/models/tagging.rb",
            "class Tagging < ApplicationRecord\n  belongs_to :article\n  belongs_to :tag\nend\n",
        )
        .write("app/models/tag.rb", "class Tag < ApplicationRecord\n  has_many :taggings\nend\n")
        .write(
            "db/schema.rb",
            r#"ActiveRecord::Schema.define do
  create_table "articles", force: :cascade do |t|
    t.string "title"
    t.integer "score"
  end
  create_table "tags", force: :cascade do |t|
    t.string "name"
  end
  create_table "taggings", force: :cascade do |t|
    t.integer "article_id"
    t.integer "tag_id"
  end
end
"#,
        )
}

/// The `has_many :through` preload and an explicit-`select` load both
/// hydrate from Hash rows; neither may widen `from_raw`'s parameter, and
/// the binary built from the same tree loads the right rows.
#[test]
#[ignore = "requires the Spinel toolchain"]
fn hash_row_hydration_keeps_from_raw_typed() {
    let (emitted, errors) = tagged_articles().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "emit errors: {errors:#?}");
    std::fs::write(
        emitted.join("contract.rb"),
        r#"require_relative "boot"
Db.configure(":memory:")
Schema.statements.each { |sql| Db.exec(sql) }
ActiveRecord.adapter = SqliteAdapter
a = Article.create!(title: "a", score: 1)
Article.create!(title: "b", score: 2)
x = Tag.create!(name: "x")
y = Tag.create!(name: "y")
Tagging.create!(article_id: a.id, tag_id: x.id)
Tagging.create!(article_id: a.id, tag_id: y.id)
puts Article.tagged.map { |r| r.tags.map { |t| t.name }.join("+") }.join(",")
puts Article.titles.map { |r| r.title }.join(",")
"#,
    )
    .expect("write consumer");
    let compiler = std::env::var("SPINEL").unwrap_or_else(|_| "spinel".into());
    let out = Command::new(&compiler)
        .args(["--rbs", ".", "--warn-widen", "-c", "contract.rb", "-o", "contract.c"])
        .current_dir(&*emitted)
        .output()
        .expect("spawn spinel");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "spinel -c failed:\n{stderr}");

    // `--warn-widen`'s report, for the failure message: what widened
    // `from_raw` / `instantiate` and through which caller.
    let widened: Vec<&str> = stderr
        .lines()
        .filter(|l| l.contains("widened") && (l.contains("from_raw") || l.contains("instantiate")))
        .collect();

    // Each Row's `from_raw` takes the Hash itself, not a boxed value.
    let c = std::fs::read_to_string(emitted.join("contract.c")).expect("read contract.c");
    for row in ["ArticleRow", "TagRow"] {
        let head = format!("sp_{row}_s_from_raw(");
        let sig = c
            .lines()
            .find(|l| l.contains(&head) && !l.trim_end().ends_with(';'))
            .or_else(|| c.lines().find(|l| l.contains(&head)))
            .unwrap_or_else(|| panic!("no {head} in contract.c"));
        let params = &sig[sig.find(&head).unwrap() + head.len()..];
        assert!(
            params.contains("sp_StrHash *") || params.contains("Hash *"),
            "{row}.from_raw takes a boxed row: {sig}\n{}",
            widened.join("\n")
        );
    }

    // The C compiles and the binary answers as Rails does.
    let built = Command::new(&compiler)
        .args(["--rbs", ".", "contract.rb", "-o", "contract"])
        .current_dir(&*emitted)
        .output()
        .expect("spawn spinel");
    assert!(built.status.success(), "spinel build failed:\n{}", String::from_utf8_lossy(&built.stderr));
    let run = Command::new(emitted.join("contract")).current_dir(&*emitted).output().expect("run contract");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "x+y,\na,b\n",
        "stderr:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

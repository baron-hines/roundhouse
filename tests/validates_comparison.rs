//! `validates :attr, comparison: { … }` (Rails' `ComparisonValidator`, activemodel 8.1): each option's bound is read
//! as it validates (a lambda of no parameters run on the class it was written in, maybe's `greater_than: -> {
//! min_supported_date }`, a class method; a Symbol the record's method; a literal), checked in Rails' option order, a nil or blank value is "can't be blank", and the message's `%{count}` is the bound
//! (`must be greater than 2020-01-01`); the last of a repeated option is checked, and an `ArgumentError` a check raises
//! is its error (`comparison of Integer with nil failed`). Ingest refused the validator ("comparison").

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const COLUMNS: &str = "create_table \"articles\", force: :cascade do |t|\n    t.date \"published_on\"\n    t.integer \"rank\"\n    t.integer \"score\"";

const MODEL: &str = r#"class Article < ApplicationRecord
  has_many :comments, dependent: :destroy
  validates :published_on, comparison: { greater_than: -> { min_date } }, allow_nil: true
  validates :rank, comparison: { less_than_or_equal_to: 10, other_than: :banned_rank }, allow_nil: true
  validates :score, comparison: { greater_than: 10, greater_than: :no_floor }, allow_nil: true

  def no_floor
    nil
  end

  # A lambda of no parameters runs on the class (Rails' `resolve_value` calls it as it is): a class method.
  def self.min_date
    Date.new(2020, 1, 1)
  end

  def banned_rank
    7
  end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("db/schema.rb", "create_table \"articles\", force: :cascade do |t|", COLUMNS)
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n  has_many :comments, dependent: :destroy\n", MODEL)
}

const SCRIPT: &str = r##"
valid = { title: "Compare", body: "A body long enough to validate." }
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
check("after the lambda's date", Article.new(**valid, published_on: Date.new(2021, 5, 1)).valid?, true)
early = Article.new(**valid, published_on: Date.new(2019, 5, 1))
check("before it", early.valid?, false)
check("message", early.errors.to_a, ["Published on must be greater than 2020-01-01"])
check("within the literal", Article.new(**valid, rank: 3).valid?, true)
high = Article.new(**valid, rank: 11)
check("over it", high.valid?, false)
check("literal message", high.errors.to_a, ["Rank must be less than or equal to 10"])
banned = Article.new(**valid, rank: 7)
check("the symbol's value", banned.valid?, false)
check("symbol message", banned.errors.to_a, ["Rank must be other than 7"])
# The last of a repeated option is the one checked; an ArgumentError of a check is its error, as Rails rescues it.
scored = Article.new(**valid, score: 5)
check("repeated option, nil bound", scored.valid?, false)
check("rescued message", scored.errors.to_a, ["Score comparison of Integer with nil failed"])
puts "comparison passed"
"##;

#[test]
fn a_comparison_reads_its_bounds_on_the_record_on_ruby() {
    let run = app().run_ruby(&format!("Db.configure(\":memory:\")\nSchema.statements.each {{ |sql| Db.exec(sql) }}\n{SCRIPT}"));
    run.assert_passes();
    assert!(run.stdout.contains("comparison passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_comparison_reads_its_bounds_on_the_record_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("comparison passed"), "{}", run.stdout);
}

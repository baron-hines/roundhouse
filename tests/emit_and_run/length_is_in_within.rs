use super::emit_and_run;

/// `length: { in: a..b }` and `{ within: a...b }` are Rails' minimum and
/// maximum, `{ is: n }` its one accepted length (wrong_length), and a
/// presence validator reads no `with:` key (lobsters' Story, Message,
/// Vote and Comment). These were survey gaps that dropped the model.
/// Answers are Rails 8.1's for the same model and records.
fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "t.text \"body\"\n    t.datetime \"created_at\", null: false\n    t.datetime \"updated_at\", null: false\n  end\n\n  create_table \"comments\"",
            "t.text \"body\"\n    t.string \"code\"\n    t.string \"note\"\n    t.string \"tag\"\n    t.datetime \"created_at\", null: false\n    t.datetime \"updated_at\", null: false\n  end\n\n  create_table \"comments\"",
        )
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true, length: { in: 3..5 }
  validates :code, length: { is: 2 }, allow_blank: true
  validates :note, length: { within: 2...4 }
  validates :tag, length: { is: 0 }
  validates :tag, presence: { with: true, message: \"cannot be empty.\" }
",
        )
}

#[test]
fn length_in_within_and_is_answer_like_rails() {
    let script = r#"rows = [
  { title: "abcd", body: "A sufficiently long body.", code: "", note: "abc", tag: nil },
  { title: "ab", body: "A sufficiently long body.", code: "x", note: "a", tag: "" },
  { title: "abcdef", body: "A sufficiently long body.", code: "xyz", note: "abcd", tag: "x" },
  { title: "abc", body: "A sufficiently long body.", code: nil, note: nil, tag: nil },
]
got = rows.map do |attrs|
  a = Article.new(attrs)
  a.valid?
  a.errors.to_a
end
want = [
  ["Tag is the wrong length (should be 0 characters)", "Tag cannot be empty."],
  ["Title is too short (minimum is 3 characters)", "Code is the wrong length (should be 2 characters)", "Note is too short (minimum is 2 characters)", "Tag cannot be empty."],
  ["Title is too long (maximum is 5 characters)", "Code is the wrong length (should be 2 characters)", "Note is too long (maximum is 3 characters)", "Tag is the wrong length (should be 0 characters)"],
  ["Note is too short (minimum is 2 characters)", "Tag is the wrong length (should be 0 characters)", "Tag cannot be empty."],
]
raise "length errors: #{got.inspect}" unless got == want
puts "length in/within/is passed"
"#;
    let run = app().run_ruby(script);
    run.assert_passes();
    assert!(run.stdout.contains("length in/within/is passed"));
}

#[test]
fn length_is_lowers_on_a_strict_target_without_errors() {
    let (_emitted, errors) = app().emit(roundhouse::project::BuildTarget::Rust);
    assert!(!errors.iter().any(|e| e.contains("length")), "{errors:#?}");
}

/// An empty exclusive range (`in: 0...0`) has no maximum, which Rails
/// rejects when the model loads; it is a gap here, not a wrapped bound.
#[test]
fn an_empty_exclusive_length_range_is_a_gap() {
    let (emitted, _errors) = emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true, length: { in: 0...0 }\n",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    let model = std::fs::read_to_string(emitted.join("app/models/article.rb")).expect("article.rb");
    assert!(!model.contains("4294967295") && !model.contains("too long"), "{model}");
}

/// An Integer attribute has no `length`; Rails measures its text
/// (`value.to_s.length`), so `42` is two long and `7` one.
#[test]
fn a_value_without_length_is_measured_by_its_text() {
    let run = emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "    t.string \"title\"\n    t.text \"body\"\n",
            "    t.string \"title\"\n    t.text \"body\"\n    t.integer \"rank\"\n",
        )
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true\n  validates :rank, length: { is: 2 }\n",
        )
        .run_ruby(
            r#"ok = Article.new(title: "T", body: "A sufficiently long body.", rank: 42)
short = Article.new(title: "T", body: "A sufficiently long body.", rank: 7)
raise "42: #{ok.valid?} #{ok.errors.to_a.inspect}" unless ok.valid?
raise "7: #{short.errors.to_a.inspect}" if short.valid?
puts "integer length passed"
"#,
        );
    run.assert_passes();
    assert!(run.stdout.contains("integer length passed"));
}

/// A Hash has `length`, its entry count, which Rails measures before
/// falling back to text.
#[test]
fn a_hash_is_measured_by_its_entries() {
    let run = emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true\n  attr_accessor :labels\n  validates :labels, length: { is: 1 }, allow_nil: true\n",
        )
        .run_ruby(
            r#"one = Article.new(title: "T", body: "A sufficiently long body.")
one.labels = { "x" => 1 }
two = Article.new(title: "T", body: "A sufficiently long body.")
two.labels = { "x" => 1, "y" => 2 }
raise "one: #{one.errors.to_a.inspect}" unless one.valid?
raise "two" if two.valid?
puts "hash length passed"
"#,
        );
    run.assert_passes();
    assert!(run.stdout.contains("hash length passed"));
}

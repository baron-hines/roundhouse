use super::emit_and_run;

/// A validation's `if: ->(record) { … }` (lobsters' Link): Rails calls a
/// one-parameter condition with the record, so its body reads the record
/// as `self`. It was a survey gap that dropped the call.
#[test]
fn a_one_parameter_condition_reads_the_record() {
    emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true
  validates :title, uniqueness: true, if: ->(a) { a.title.start_with?(\"U\") }
",
        )
        .run_ruby(r#"Article.delete_all
Article.create!(title: "Uno", body: "A sufficiently long body.")
Article.create!(title: "Dos", body: "A sufficiently long body.")
taken = Article.new(title: "Uno", body: "A sufficiently long body.")
raise "guarded: #{taken.errors.to_a.inspect}" if taken.valid?
raise "message: #{taken.errors.to_a.inspect}" unless taken.errors.to_a == ["Title has already been taken"]
free = Article.new(title: "Dos", body: "A sufficiently long body.")
raise "unguarded: #{free.errors.to_a.inspect}" unless free.valid?
puts "condition lambda passed"
"#)
        .assert_passes();
}

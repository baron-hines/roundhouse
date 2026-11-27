use super::emit_and_run;

/// `format: { with: /\A#{NAME}\z/o }` (lobsters' UsernameAttribute):
/// `/o` evaluates the interpolation once, which for a folded constant is
/// the pattern evaluating it each time builds. It was a survey gap.
#[test]
fn a_once_interpolated_constant_pattern_validates_like_rails() {
    emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  VALID_NAME = /[A-Za-z0-9][A-Za-z0-9_-]{0,4}/
  validates :title, presence: true, format: { with: /\\A#{VALID_NAME}\\z/o }
",
        )
        .run_ruby(r#"got = ["ab_c", "-ab", "abcdefg", "A1"].map do |title|
  a = Article.new(title: title, body: "A sufficiently long body.")
  a.valid?
  a.errors.to_a
end
want = [[], ["Title is invalid"], ["Title is invalid"], []]
raise "format: #{got.inspect}" unless got == want
puts "format once passed"
"#)
        .assert_passes();
}

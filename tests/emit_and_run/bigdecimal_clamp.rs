use super::emit_and_run;

/// Comparable's `clamp` and `between?` on a BigDecimal (lobsters'
/// Comment#calculated_confidence clamps a BigDecimal to `0..1`). `clamp`
/// answers the receiver or the bound it fell past, which keeps its own
/// class. Answers are CRuby 3.4's.
#[test]
fn a_bigdecimal_clamps_and_compares_like_ruby() {
    emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true

  def self.clamped(text)
    BigDecimal(text).clamp(0..1)
  end

  def self.inside?(text)
    BigDecimal(text).between?(0, 1)
  end
",
        )
        .run_ruby(r#"got = [Article.clamped("1.5"), Article.clamped("-0.25"), Article.clamped("0.5"), Article.inside?("0.5"), Article.inside?("2")]
raise "clamp: #{got.inspect}" unless got.map(&:inspect) == ["1", "0", "0.5e0", "true", "false"]
puts "bigdecimal clamp passed"
"#)
        .assert_passes();
}

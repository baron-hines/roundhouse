//! One `String#truncate_words` contract for the interpreted lane.
//! ActiveSupport reopens `String` for it, which the transpiled runtime
//! cannot host, so a String-typed call is grounded to
//! `ActiveSupport.truncate_words`. Before that grounding the call was
//! emitted as written and raised NoMethodError. The expected values are
//! what Rails 8.1.4 answers for the same calls.

pub const SOURCE: &str = r#"class StringTruncateWordsProbe
  TEXT = "one two three four".freeze

  def self.results
    text = TEXT
    [
      text.truncate_words(2),
      text.truncate_words(2, omission: " (more)"),
      text.truncate_words(10),
      "x y z".truncate_words(2, omission: " (more)"),
      "  leading words here".truncate_words(2),
      "".truncate_words(3),
    ]
  end
end
"#;

pub const ASSERTIONS: &str = r#"expected = [
  "one two...",
  "one two (more)",
  "one two three four",
  "x y (more)",
  "  leading...",
  "",
]
actual = StringTruncateWordsProbe.results
raise "String#truncate_words parity: #{actual.inspect}" unless actual == expected
puts "String truncate_words emitted contract passed"
"#;

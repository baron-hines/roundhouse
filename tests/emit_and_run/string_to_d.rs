//! maybe's `params[:amount].to_d` and `Import::Row#signed_amount`'s
//! `price.to_d`: bigdecimal/util's `String#to_d` reads the longest leading
//! decimal (`"12.3abc"` is 12.3, `"abc"` and `""` are 0, `"1_000.5"` drops
//! the `_`), `nil.to_d` is 0, a value the analysis could not type takes the
//! Numeric reopens' `to_d` too, and the runtime spells them as Ruby answers
//! them, on CRuby and on Spinel (but a negative zero and Infinity, which
//! Spinel's BigDecimal package does not represent).

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit(
        "app/models/article.rb",
        "class Article < ApplicationRecord\n",
        "class Article < ApplicationRecord\n  def title_d = title.to_d\n\n  #: (String) -> BigDecimal\n  def self.loose(text) = text.to_d\n\n  def self.maybe(flag) = (flag ? \"7.25\" : nil).to_d\n\n  def self.any(value) = value.to_d\n",
    )
}

const ASSERTIONS: &str = r##"
def expect(label, got, want)
  raise "#{label}: got #{got.inspect}, want #{want.inspect}" unless got == want
end
expect("title", Article.new(title: "12.3abc", body: "a body long enough").title_d.to_s, "0.123e2")
{ "12.3" => "0.123e2", "abc" => "0.0", "" => "0.0", "1_000.5" => "0.10005e4", "-0.5" => "-0.5e0", "1e3" => "0.1e4", ".5" => "0.5e0", "1__0" => "0.1e1", "1e" => "0.1e1", "+NaN" => "0.0", "12é" => "0.12e2" }.each do |text, want|
  expect(text, Article.loose(text).to_s, want)
end
expect("present", Article.maybe(true).to_s, "0.725e1")
expect("nil", Article.maybe(false).to_s, "0.0")
expect("sum", (Article.loose("1.1") + Article.loose("2.2")).to_s, "0.33e1")
expect("untyped int", Article.any(3).to_s, "0.3e1")
expect("untyped float", Article.any(0.1).to_s, "0.1e0")
expect("untyped decimal", Article.any(BigDecimal("2.5")).to_s, "0.25e1")
expect("untyped string", Article.any("4.5x").to_s, "0.45e1")
puts "to_d passed"
"##;

/// What only CRuby's BigDecimal represents: a negative zero, Infinity. Spinel's
/// package has neither (its header lists them), so these stay CRuby-only.
const CRUBY_ONLY: &str = r##"
{ "--1" => "-0.0", "-0" => "-0.0", "Infinity" => "Infinity", "-Infinity" => "-Infinity" }.each do |text, want|
  got = Article.loose(text).to_s
  raise "#{text}: got #{got.inspect}, want #{want.inspect}" unless got == want
end
"##;

#[test]
fn string_to_d_reads_as_ruby_does() {
    let run = app().run_ruby(&format!("{CRUBY_ONLY}{ASSERTIONS}"));
    run.assert_passes();
    assert!(run.stdout.contains("to_d passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn string_to_d_reads_as_ruby_does_on_spinel() {
    let run = app().run_spinel_with_db(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("to_d passed"), "{}", run.stdout);
}

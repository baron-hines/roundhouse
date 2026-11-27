//! `n.times` without a block, chained (maybe's `8.times.map { SecureRandom.hex(4) }`
//! in a model): the enumerator's values flow, 0...n, as Ruby's do.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit(
        "app/models/article.rb",
        "class Article < ApplicationRecord\n",
        "class Article < ApplicationRecord\n  def backup_codes\n    8.times.map { SecureRandom.hex(4) }\n  end\n\n  def doubled\n    3.times.map { |i| i * 2 }\n  end\n\n  def indexes = 4.times.to_a\n",
    )
}

const ASSERTIONS: &str = r#"a = Article.new
codes = a.backup_codes
raise codes.inspect unless codes.length == 8 && codes.all? { |c| c.length == 8 }
raise a.doubled.inspect unless a.doubled == [0, 2, 4]
raise a.indexes.inspect unless a.indexes == [0, 1, 2, 3]
puts "times enumerator passed"
"#;

#[test]
fn a_blockless_times_chains_its_values() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("times enumerator passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_blockless_times_chains_its_values_on_spinel() {
    let run = app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("times enumerator passed"), "{}", run.stdout);
}

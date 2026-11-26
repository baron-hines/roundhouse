//! A gem's own deprecator (`Devise.deprecator`): `ActiveSupport::Deprecation#warn` writes Rails'
//! line to stderr, and a silenced one writes nothing.

use super::emit_and_run;

const METHODS: &str = r#"
  def self.deprecator
    @deprecator ||= ActiveSupport::Deprecation.new("5.0", "Blog")
  end

  def self.warn_twice
    deprecator.warn("Article.old is deprecated")
    quiet = ActiveSupport::Deprecation.new("5.0", "Blog")
    quiet.silenced = true
    quiet.warn("never written")
    [deprecator.gem_name, deprecator.deprecation_horizon]
  end
"#;

const SCRIPT: &str = r#"got = Article.warn_twice
raise got.inspect unless got == ["Blog", "5.0"]
puts "deprecation passed"
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit("app/models/article.rb", "class Article < ApplicationRecord\n", &format!("class Article < ApplicationRecord\n{METHODS}"))
}

fn assert_warned(run: &emit_and_run::Run) {
    assert!(run.stdout.contains("deprecation passed"), "{}", run.stdout);
    assert!(run.stderr.contains("DEPRECATION WARNING: Article.old is deprecated"), "{}", run.stderr);
    assert!(!run.stderr.contains("never written"), "{}", run.stderr);
}

#[test]
fn a_deprecator_warns_unless_silenced() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert_warned(&run);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_deprecator_warns_unless_silenced_natively() {
    let run = app().run_spinel(SCRIPT);
    run.assert_passes();
    assert_warned(&run);
}

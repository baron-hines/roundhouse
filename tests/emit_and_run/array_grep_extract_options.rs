//! Devise's controller helpers: `options = args.extract_options!` (ActiveSupport), which takes the trailing
//! Hash off the Array it is called on, and `session.keys.grep(/\Adevise\./)`.

use super::emit_and_run;

const MODEL: &str = r#"
  def self.split_options(*args)
    options = args.extract_options!
    [args, options]
  end

  def self.split_names(*names)
    options = names.extract_options!
    [names, options]
  end

  def self.named_split
    split_names("a", "b")
  end

  def self.devise_keys(keys)
    keys.grep(/\Adevise\./)
  end

  def self.other_keys(keys)
    keys.grep_v(/\Adevise\./)
  end
"#;

const SCRIPT: &str = r#"got = Article.split_options(:a, :b, scope: :user)
raise got.inspect unless got == [[:a, :b], { scope: :user }]
got = Article.split_options(:a)
raise got.inspect unless got == [[:a], {}]
got = Article.named_split
raise got.inspect unless got == [["a", "b"], {}]
keys = ["devise.mapping", "warden", "devise.skip"]
raise Article.devise_keys(keys).inspect unless Article.devise_keys(keys) == ["devise.mapping", "devise.skip"]
raise Article.other_keys(keys).inspect unless Article.other_keys(keys) == ["warden"]
puts "array helpers passed"
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit("app/models/article.rb", "class Article < ApplicationRecord\n", &format!("class Article < ApplicationRecord\n{MODEL}"))
}

#[test]
fn extract_options_and_grep_run_as_rails_does() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("array helpers passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn extract_options_and_grep_run_as_rails_does_natively() {
    let run = app().run_spinel(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("array helpers passed"), "{}", run.stdout);
}

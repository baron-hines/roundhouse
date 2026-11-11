//! `self.class` handed to another method is the class, not an instance of it: the parameter takes the class object,
//! whose class methods the callee calls (Devise's `Devise::Encryptor.digest(self.class, password)`, which reads
//! `klass.pepper` and `klass.stretches`).

use super::emit_and_run;

const STAMP: &str = r##"module Stamp
  def self.label(klass, text)
    "#{klass.prefix}:#{text}"
  end
end
"##;

const MODEL: &str = r#"class Article < ApplicationRecord
  def self.prefix
    "article"
  end

  def stamped
    Stamp.label(self.class, title)
  end

"#;

const SCRIPT: &str = r#"article = Article.new(title: "Hello")
raise "stamped #{article.stamped.inspect}" unless article.stamped == "article:Hello"
puts "self class argument passed"
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/stamp.rb", STAMP).edit("app/models/article.rb", "class Article < ApplicationRecord\n", MODEL)
}

#[test]
fn self_class_as_an_argument_is_the_class() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("self class argument passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn self_class_as_an_argument_is_the_class_natively() {
    let run = app().run_spinel(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("self class argument passed"), "{}", run.stdout);
}

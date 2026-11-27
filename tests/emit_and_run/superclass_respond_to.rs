//! Devise's `Models.config` readers, copied onto each model's class side:
//! `if defined?(@x) … elsif superclass.respond_to?(:x) superclass.x else
//! Devise.x end`. A model nothing subclasses has one superclass, whose
//! class side answers no `x`, so that branch never runs: Ruby asks the
//! module's default, and the tree reads it without the call it could not
//! type (`superclass.x` on a parent that has none).

use super::emit_and_run;

const CONCERN: &str = r#"module Labeled
  extend ActiveSupport::Concern

  def self.default_label
    "default"
  end

  module ClassMethods
    def label
      if defined?(@label)
        @label
      elsif superclass.respond_to?(:label)
        superclass.label
      else
        Labeled.default_label
      end
    end

    def label=(value)
      @label = value
    end
  end
end
"#;

const SCRIPT: &str = r#"raise "unset #{Article.label.inspect}" unless Article.label == "default"
Article.relabel!
raise "set #{Article.label.inspect}" unless Article.label == "set"
puts "superclass respond_to passed"
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/models/concerns/labeled.rb", CONCERN)
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  include Labeled\n\n  def self.relabel!\n    self.label = \"set\"\n  end\n\n")
}

#[test]
fn a_parent_that_answers_no_config_reader_falls_back_to_the_default() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("superclass respond_to passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_parent_that_answers_no_config_reader_falls_back_to_the_default_natively() {
    let run = app().run_spinel(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("superclass respond_to passed"), "{}", run.stdout);
}

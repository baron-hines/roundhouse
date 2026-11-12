//! devise's `devise :database_authenticatable`: a class macro a gem's module gives every model through
//! `ActiveSupport.on_load(:active_record) { extend Devise::Models }`, whose body includes modules into the
//! model (running each one's `included` hook, as Ruby's `include` does) and sets a class attribute. The
//! modules it includes are the model's own, as a written `include` makes them.

use super::emit_and_run;

const KIT: &str = r##"module Badges
  module Orm
    def self.included(model)
      model.include Dirty if Badges::Orm.recent?(model)
    end

    def self.recent?(model)
      true
    end

    module Dirty
      def dirty_badge
        "dirty #{title}"
      end
    end
  end

  module Shiny
    extend ActiveSupport::Concern

    included do
      class_attribute :badge_modules, instance_writer: false
      self.badge_modules ||= []
    end

    def shiny_badge
      "shiny #{title}"
    end
  end

  module Macros
    def badges(*names)
      selected = names.map(&:to_sym).uniq.sort_by { |n| ORDER.index(n) || -1 }
      include Badges::Orm
      include Badges::Shiny
      self.badge_modules |= selected
    end
  end

  ORDER = [:shiny, :plain].freeze
end

ActiveSupport.on_load(:active_record) do
  extend Badges::Macros
end
"##;

const SCRIPT: &str = r#"article = Article.new(title: "t")
raise "shiny" unless article.shiny_badge == "shiny t"
raise "dirty" unless article.dirty_badge == "dirty t"
raise "modules #{Article.badge_modules.inspect}" unless Article.badge_modules == [:shiny, :plain]
puts "model mixin macro passed"
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("lib/badges.rb", KIT)
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  badges :plain, :shiny\n")
}

#[test]
fn a_model_macro_that_mixes_in_includes_its_modules() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("model mixin macro passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_model_macro_that_mixes_in_includes_its_modules_natively() {
    let run = app().run_spinel(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("model mixin macro passed"), "{}", run.stdout);
}

/// A module whose `included` hook declares a validation on the including model, reached through a macro on one
/// model and, on the other, either a macro call the expansion cannot run or a written `include`. The hook's
/// declarations are only what the expansion compiles, so neither model is rewritten (the hook would run again on
/// the rewritten one's `include`), and each use is reported unsupported rather than left to fail at load.
const STAMPS: &str = r##"module Stamps
  module Short
    def self.included(base)
      base.class_eval do
        validates_length_of :body, maximum: 20
      end
    end
  end

  module Macros
    def stamps(*names)
      include Stamps::Short
    end
  end
end

ActiveSupport.on_load(:active_record) do
  extend Stamps::Macros
end
"##;

fn stamps(comment_line: &str) -> Vec<String> {
    let (_, errors) = emit_and_run::real_blog()
        .write("lib/stamps.rb", STAMPS)
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  stamps :short\n")
        .edit("app/models/comment.rb", "class Comment < ApplicationRecord\n", &format!("class Comment < ApplicationRecord\n  {comment_line}\n"))
        .emit(roundhouse::build_target::BuildTarget::Ruby);
    errors
}

#[test]
fn a_hook_beside_a_macro_call_the_expansion_cannot_run_is_reported() {
    let errors = stamps("stamps(*SecureRandom.hex(1).chars.first(0))");
    assert!(errors.iter().any(|e| e.contains("Stamps::Short") && e.contains("unexpanded")), "{errors:#?}");
}

#[test]
fn a_hook_beside_a_model_that_includes_its_module_is_reported() {
    let errors = stamps("include Stamps::Short");
    assert!(errors.iter().any(|e| e.contains("Stamps::Short") && e.contains("unexpanded")), "{errors:#?}");
    assert!(errors.iter().any(|e| e.contains("`Stamps::Short.included` declares on the model at load")), "{errors:#?}");
}

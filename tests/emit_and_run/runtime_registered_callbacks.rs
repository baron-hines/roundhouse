//! Callbacks a gem's `included(base)` hook registers as it runs (aasm's `base.after_initialize`), as Rails 8.1.4's
//! `ActiveSupport::Callbacks` runs them: a Proc taking an argument gets the record, a Symbol registered again is the
//! one entry with the later options, and a subclass whose own hook registers more runs its parent's once.

use super::emit_and_run;

const GEM: &str = "vendor/bundle/ruby/3.4.0/gems/stampy-0.1.0/lib";

const HOOK: &str = r##"module Stampy
  def self.included(base)
    base.before_save :mark, if: :never?
    base.before_save :mark
    base.before_save ->(record) { record.title = record.title.upcase }
    base.before_save { self.body = body + " (stamped)" }
    base.after_initialize :count_init, unless: ->(record) { record.persisted? }
  end

  def mark
    self.title = "#{title}!"
  end

  def never?
    false
  end

  def count_init
    @inits = (@inits || 0) + 1
  end

  def inits
    @inits
  end
end
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    stampy (0.1.0)\n    debug (1.11.1)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  stampy\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write("roundhouse.yml", "extensions:\n  - config/roundhouse\n")
        .write("config/roundhouse/gems.yml", "source_gems:\n  - {gem: \"stampy\"}\n")
        .write(&format!("{GEM}/stampy.rb"), HOOK)
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  include Stampy\n")
}

#[test]
fn hook_registered_callbacks_run_as_rails_runs_them() {
    app()
        .run_ruby(
            r#"
a = Article.create!(title: "hello", body: "a body long enough")
got = [a.title, a.body, a.inits, Article.find(a.id).inits]
want = ["HELLO!", "a body long enough (stamped)", 1, nil]
raise "got #{got.inspect}" unless got == want
puts "ok"
"#,
        )
        .assert_passes();
}

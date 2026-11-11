//! Branches Ruby never takes.
//!
//! - A predicate whose body is a literal, and whose name nothing else defines, answers that constant, so `if
//!   x.beta?` takes one branch and the other, typed, would report what never runs (Devise's
//!   `if devise_mapping.validatable?` over a model that is not `:validatable`), and so does an `&&` or `||` its
//!   left settles (`devise_mapping.lockable? && resource_class.unlock_strategy_enabled?(:email)`).
//! - `x.try(:m)` on a class nothing in whose chain answers `m` is nil, so its `if` takes the `else` (Devise's
//!   `@resource.try(:unconfirmed_email?)` on a model that is not `:confirmable`).
//!   In a template the branch it takes renders in the `if`'s place (Devise's `email_changed` mail).
//! - `instance_variable_set` answers the value it sets.
//! - A gem method no reached call passes a block to never takes its `x if block_given?` (Warden's `Strategies.add`,
//!   called with the strategy class alone once the `update` nothing calls is left out).

use super::emit_and_run;

const FLAGS: &str = r#"class Flags
  def beta?
    false
  end

  def stable?
    true
  end
end
"#;

const MODEL: &str = r#"class Article < ApplicationRecord
  def self.release(flags)
    if flags.beta?
      flags.preview_build
    else
      "stable"
    end
  end

  def self.release_of_default
    release(Flags.new)
  end

  def self.gated(flags)
    if flags.beta? && flags.preview_build.ready?
      flags.preview_build
    elsif flags.beta? || flags.stable?
      "stable"
    else
      "none"
    end
  end

  def self.gated_of_default
    gated(Flags.new)
  end

  def self.pending(flags)
    if flags.try(:pending_build)
      flags.pending_build.label
    else
      "none pending"
    end
  end

  def self.pending_of_default
    pending(Flags.new)
  end

"#;

// Ruby only: Spinel refuses `instance_variable_set` itself.
const STAMPED: &str = "  def self.stamped\n    @stamp = instance_variable_set(:@stamped_at, \"now\")\n    @stamp.upcase\n  end\n\n";

const SCRIPT: &str = r#"raise "release #{Article.release_of_default.inspect}" unless Article.release_of_default == "stable"
raise "gated #{Article.gated_of_default.inspect}" unless Article.gated_of_default == "stable"
raise "pending #{Article.pending_of_default.inspect}" unless Article.pending_of_default == "none pending"
puts "constant branches passed"
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/flags.rb", FLAGS).edit("app/models/article.rb", "class Article < ApplicationRecord\n", MODEL)
}

#[test]
fn a_constant_predicate_takes_one_branch() {
    let run = app()
        .edit("app/models/article.rb", "  def self.release_of_default\n", &format!("{STAMPED}  def self.release_of_default\n"))
        .run_ruby(&format!("{SCRIPT}raise \"stamped\" unless Article.stamped == \"NOW\"\n"));
    run.assert_passes();
    assert!(run.stdout.contains("constant branches passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_constant_predicate_takes_one_branch_natively() {
    let run = app().run_spinel(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("constant branches passed"), "{}", run.stdout);
}

const REGISTRY: &str = r#"module Kinds
  class Base
    def run
      "base"
    end
  end

  class Plain < Base
    def run
      "plain"
    end
  end

  class Registry
    @@kinds = {}

    def self.add(label, kind, &block)
      kind.class_eval(&block) if block_given?
      @@kinds[label] = kind
    end

    def self.update(label, &block)
      add(label, @@kinds[label], &block)
    end

    def self.run(label)
      @@kinds[label].new.run
    end
  end
end
"#;

fn installed() -> emit_and_run::Overlay {
    let gem = "vendor/bundle/ruby/3.4.0/gems/kinds-1.0";
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    debug (1.11.1)\n    kinds (1.0)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  kinds\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write(&format!("{gem}/lib/kinds.rb"), "require \"kinds/registry\"\n")
        .write(&format!("{gem}/lib/kinds/registry.rb"), REGISTRY)
        .write("config/initializers/kinds.rb", "Kinds::Registry.add(:plain, Kinds::Plain)\n")
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  def self.kind_run\n    Kinds::Registry.run(:plain)\n  end\n\n")
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_gem_method_no_call_passes_a_block_leaves_its_block_given_branch_out() {
    let run = installed().run_spinel("raise \"run\" unless Article.kind_run == \"plain\"\nputs \"block_given passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("block_given passed"), "{}", run.stdout);
}

const NOTE_CONTROLLER: &str = "class NotesController < ApplicationController\n  def show\n    @article = Article.first\n  end\nend\n";

const NOTE_VIEW: &str = "<p>Note</p>\n<% if @article.try(:archived?) %>\n  <p>Archived <%= @article.archived_at %></p>\n<% else %>\n  <p>Live <%= @article.title %></p>\n<% end %>\n";

const NOTE_TEST: &str = r#"require "test_helper"

class NotesControllerTest < ActionDispatch::IntegrationTest
  test "the branch a settled try takes renders" do
    get "/note"
    assert_response :success
    assert_includes response.body, "Live #{Article.first.title}"
    assert_not_includes response.body, "Archived"
  end
end
"#;

#[test]
fn a_settled_try_in_a_template_renders_the_branch_it_takes() {
    emit_and_run::real_blog()
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  get \"/note\", to: \"notes#show\"\n")
        .write("app/controllers/notes_controller.rb", NOTE_CONTROLLER)
        .write("app/views/notes/show.html.erb", NOTE_VIEW)
        .write("test/controllers/notes_controller_test.rb", NOTE_TEST)
        .run_test("test/controllers/notes_controller_test.rb")
        .assert_passes();
}

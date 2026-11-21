//! What maybe's emitted Ruby tree met as it booted, each answered as Rails 8.1.4 does.

use super::emit_and_run;

/// `include Turbo::FramesHelper, Turbo::StreamsHelper` in a class (maybe's `ApplicationComponent`): turbo-rails'
/// modules, which a template's `turbo_frame_tag` reads, exist to include.
#[test]
fn turbo_rails_helper_modules_can_be_included() {
    let run = emit_and_run::real_blog()
        .write("app/models/frame_probe.rb", "class FrameProbe\n  include Turbo::FramesHelper, Turbo::StreamsHelper\n\n  def label\n    \"framed\"\n  end\nend\n")
        .run_ruby("raise FrameProbe.new.label unless FrameProbe.new.label == \"framed\"\nputs \"turbo modules passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("turbo modules passed"), "{}", run.stdout);
}

/// The gem's own hierarchy: `I18n::InvalidLocale` is an `I18n::ArgumentError`, which `rescue I18n::ArgumentError`
/// catches, and an `::ArgumentError` too.
#[test]
fn i18n_errors_share_the_gems_argument_error() {
    let run = emit_and_run::real_blog()
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  def locale_name\n    I18n.locale.to_s\n  end\n\n")
        .run_ruby(
        "got = [I18n::InvalidLocale.ancestors.include?(I18n::ArgumentError), I18n::ArgumentError.superclass]\n\
         raise \"got #{got.inspect}\" unless got == [true, ::ArgumentError]\nputs \"i18n errors passed\"\n",
    );
    run.assert_passes();
    assert!(run.stdout.contains("i18n errors passed"), "{}", run.stdout);
}

/// maybe's `lib/money.rb`: an app `lib/` class of its own that includes `ActiveModel::Validations` validates as
/// Rails' does: `validate :method` runs on `valid?`, and `validate!` raises `ActiveModel::ValidationError`.
#[test]
fn an_app_lib_class_with_active_model_validations_validates() {
    let run = emit_and_run::real_blog()
        .write(
            "lib/amount.rb",
            "class Amount\n  include ActiveModel::Validations\n\n  attr_reader :value\n\n  validate :value_must_be_positive\n\n  def initialize(value)\n    @value = value\n  end\n\n  def self.checked(value)\n    amount = new(value)\n    amount.validate!\n    amount\n  end\n\n  private\n    def value_must_be_positive\n      errors.add(:value, \"must be positive\") if value < 0\n    end\nend\n",
        )
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  def amount_ok?(n)\n    Amount.new(n).valid?\n  end\n\n")
        .run_ruby(
            r#"a = Article.new
got = [a.amount_ok?(1), a.amount_ok?(-1), Amount.checked(2).value]
begin
  Amount.checked(-2)
  got << :no_raise
rescue ActiveModel::ValidationError => e
  got << e.message
end
raise "got #{got.inspect}" unless got == [true, false, 2, "Validation failed: Value must be positive"]
puts "lib validations passed"
"#,
        );
    run.assert_passes();
    assert!(run.stdout.contains("lib validations passed"), "{}", run.stdout);
}

/// rails-settings-cached's `self.table_name = table_name_prefix + "settings"`: the class's prefix (none here) in front
/// of the literal.
#[test]
fn a_table_name_built_on_the_prefix_names_the_table() {
    let run = emit_and_run::real_blog()
        .write("app/models/note_alias.rb", "class NoteAlias < ApplicationRecord\n  self.table_name = table_name_prefix + \"articles\"\nend\n")
        .run_ruby("Article.create!(title: \"Hello\", body: \"A body long enough\")\nraise NoteAlias.count.to_s unless NoteAlias.count == 1\nputs \"prefixed table passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("prefixed table passed"), "{}", run.stdout);
}

/// A gem the app depends on that depends on railties (a Rails plugin, rails-settings-cached): requiring it loads
/// Rails, which no emitted tree carries, so the app's reference to its constant is reported, not left to fail at boot.
#[test]
fn a_rails_plugin_gems_constant_is_reported() {
    let lock = std::fs::read_to_string(roundhouse::fixtures::real_blog().join("Gemfile.lock")).unwrap()
        .replacen("\n    rack (", "\n    probe-engine (1.0.0)\n      railties (>= 7.0)\n    rack (", 1)
        .replacen("DEPENDENCIES\n", "DEPENDENCIES\n  probe-engine\n", 1);
    let errors = emit_and_run::real_blog()
        .write("Gemfile.lock", &lock)
        .write("app/models/plugin_user.rb", "class PluginUser < ProbeEngine::Base\nend\n")
        .check_errors();
    assert!(errors.iter().any(|e| e.contains("ProbeEngine::Base") && e.contains("railties")), "{errors:#?}");
}

/// A record's `validate!` raises the `RecordInvalid` that `save!` does, as Rails' does, and answers true when valid.
#[test]
fn a_records_validate_bang_raises_record_invalid() {
    let run = emit_and_run::real_blog()
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  def checked\n    validate!\n  end\n\n")
        .run_ruby(
            r#"got = [Article.new(title: "Hello", body: "A body long enough").checked]
begin
  Article.new(title: "", body: "A body long enough").checked
  got << :no_raise
rescue ActiveRecord::RecordInvalid => e
  got << e.class.name
end
raise "got #{got.inspect}" unless got == [true, "ActiveRecord::RecordInvalid"]
puts "record validate! passed"
"#,
        );
    run.assert_passes();
    assert!(run.stdout.contains("record validate! passed"), "{}", run.stdout);
}

/// method_source's `source_location.rb`: a load-time conditional whose branches `include` a module, or define a
/// method and make it `private`, runs the branch Ruby takes (no `Proc#__file__` on this Ruby).
#[test]
fn a_load_time_conditionals_include_and_private_run_in_its_branch() {
    let run = emit_and_run::real_blog()
        .write(
            "app/models/located.rb",
            "module Located\n  module Ree\n    def where_from\n      \"ree\"\n    end\n  end\n\n  module Methods\n    if Proc.method_defined? :__file__\n      include Ree\n    else\n      def where_from\n        helper\n      end\n\n      def helper\n        \"mri\"\n      end\n\n      private :helper\n    end\n  end\nend\n",
        )
        .write("app/models/located_probe.rb", "class LocatedProbe\n  include Located::Methods\nend\n")
        .run_ruby(
            "got = [LocatedProbe.new.where_from, LocatedProbe.private_method_defined?(:helper)]\nraise \"got #{got.inspect}\" unless got == [\"mri\", true]\nputs \"conditional branch passed\"\n",
        );
    run.assert_passes();
    assert!(run.stdout.contains("conditional branch passed"), "{}", run.stdout);
}

/// Ruby's value constants a view reads (maybe's `2 * Math::PI * radius`, `Date::ABBR_MONTHNAMES.compact`).
#[test]
fn core_value_constants_read_as_rubys() {
    let run = emit_and_run::real_blog()
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  def circle(radius)\n    (2 * Math::PI * radius).round(2)\n  end\n\n  def months\n    Date::ABBR_MONTHNAMES.compact.first(2).join(\",\") + \"/\" + Date::DAYNAMES.first\n  end\n\n")
        .run_ruby("a = Article.new\ngot = [a.circle(1), a.months]\nraise \"got #{got.inspect}\" unless got == [6.28, \"Jan,Feb/Sunday\"]\nputs \"core constants passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("core constants passed"), "{}", run.stdout);
}

/// A branch that `include`s a nested module declared after the conditional, under a guard that asks for it, is
/// skipped as Ruby skips it: the module is not written ahead of the guard.
#[test]
fn a_guard_asking_for_a_later_nested_module_keeps_its_branch() {
    let run = emit_and_run::real_blog()
        .write(
            "app/models/guarded_owner.rb",
            "class GuardedOwner\n  if const_defined?(:Feature, false)\n    include Feature\n\n    def selected\n      true\n    end\n  end\n\n  module Feature\n    def feature\n      \"f\"\n    end\n  end\nend\n",
        )
        .run_ruby(
            "got = [GuardedOwner.method_defined?(:selected), GuardedOwner.include?(GuardedOwner::Feature)]\nraise \"got #{got.inspect}\" unless got == [false, false]\nputs \"guard passed\"\n",
        );
    run.assert_passes();
    assert!(run.stdout.contains("guard passed"), "{}", run.stdout);
}

/// Only the module the guard asks for stays after it: `Other`, which the other branch includes, is still written
/// ahead of the conditional, as the source defines it there.
#[test]
fn a_guard_keeps_back_only_the_module_it_asks_for() {
    let run = emit_and_run::real_blog()
        .write(
            "app/models/guarded_pair.rb",
            "class GuardedPair\n  module Other\n    def other\n      \"o\"\n    end\n  end\n\n  if const_defined?(:Feature, false)\n    include Feature\n  else\n    include Other\n  end\n\n  module Feature\n    def feature\n      \"f\"\n    end\n  end\nend\n",
        )
        .run_ruby(
            "got = [GuardedPair.new.other, GuardedPair.include?(GuardedPair::Feature)]\nraise \"got #{got.inspect}\" unless got == [\"o\", false]\nputs \"pair passed\"\n",
        );
    run.assert_passes();
    assert!(run.stdout.contains("pair passed"), "{}", run.stdout);
}

/// A module the guarded branch includes, which needs the asked-for one through another (`A` includes `B`, which
/// includes `Feature`), waits with it: writing `A` first would need `B` and so `Feature` too.
#[test]
fn a_module_needing_the_asked_one_through_another_waits_too() {
    let run = emit_and_run::real_blog()
        .write(
            "app/models/guarded_chain.rb",
            "class GuardedChain\n  if const_defined?(:Feature, false)\n    include A\n  end\n\n  module Feature\n    def feature\n      \"f\"\n    end\n  end\n\n  module B\n    include Feature\n\n    def b\n      \"b\"\n    end\n  end\n\n  module A\n    include B\n\n    def a\n      \"a\"\n    end\n  end\nend\n",
        )
        .run_ruby("got = [GuardedChain.include?(GuardedChain::A), GuardedChain::A.include?(GuardedChain::Feature)]\nraise \"got #{got.inspect}\" unless got == [false, true]\nputs \"chain passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("chain passed"), "{}", run.stdout);
}

/// `private :helper` in a branch settled at load (`defined?` of a class the app carries) makes the branch's
/// `helper` private, as Ruby runs it there.
#[test]
fn a_settled_branchs_private_marker_hides_its_method() {
    let run = emit_and_run::real_blog()
        .write(
            "app/models/settled_helpers.rb",
            "module SettledHelpers\n  if defined?(Article)\n    def helper\n      \"ok\"\n    end\n    private :helper\n\n    def open_one\n      helper\n    end\n  end\nend\n",
        )
        .write("app/models/settled_probe.rb", "class SettledProbe\n  include SettledHelpers\nend\n")
        .run_ruby(
            "got = [SettledProbe.new.open_one, SettledProbe.private_method_defined?(:helper), SettledProbe.public_method_defined?(:helper)]\nraise \"got #{got.inspect}\" unless got == [\"ok\", true, false]\nputs \"settled private passed\"\n",
        );
    run.assert_passes();
    assert!(run.stdout.contains("settled private passed"), "{}", run.stdout);
}

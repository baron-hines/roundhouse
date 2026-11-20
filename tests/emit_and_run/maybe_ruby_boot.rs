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

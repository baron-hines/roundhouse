//! A model that `extend`s an app module and calls its methods in the
//! class body (discourse's `SiteSetting`: `extend SiteSettingExtension`,
//! `load_settings(…)`, which calls `define_singleton_method(name)` for
//! each key of a YAML file) runs that code as it loads, on the Ruby
//! family as in Rails, among its own methods in source order. A call
//! the source does not declare then answers untyped there: the code
//! defines it. A condition or loop is replayed whole; one the replay
//! cannot carry keeps the class closed. A compiled target reports the replayed statements and
//! the computed definition.

use super::emit_and_run;
use roundhouse::project::BuildTarget;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "lib/settings_extension.rb",
            "module SettingsExtension\n  def setting(name, default)\n    define_singleton_method(name) { default }\n  end\n\n  if defined?(Rails::Console)\n    def info(name)\n      name\n    end\n  end\nend\n",
        )
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true\n  extend SettingsExtension\n  setting :title_limit, 40\n\n  def self.limit\n    Article.title_limit\n  end\n",
        )
}

#[test]
fn a_computed_name_reads_untyped_and_runs() {
    let errors = app().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
    app()
        .run_ruby("raise Article.limit.inspect unless Article.limit == 40\nraise \"console\" if SettingsExtension.method_defined?(:info)\nputs \"open class passed\"\n")
        .assert_passes();
}

#[test]
fn a_computed_name_is_unsupported_on_strict_targets() {
    let (_emitted, errors) = app().emit(BuildTarget::Rust);
    assert!(errors.iter().any(|e| e.contains("`define_singleton_method` with a name computed when it runs")), "{errors:#?}");
}

/// Ruby order: a method defined after the call overrides what it defines.
#[test]
fn a_def_after_the_call_overrides_it() {
    app()
        .edit("app/models/article.rb", "  def self.limit\n", "  def self.title_limit\n    99\n  end\n\n  def self.limit\n")
        .run_ruby("raise Article.limit.inspect unless Article.limit == 99\nputs \"order passed\"\n")
        .assert_passes();
}

/// A loop over a literal is replayed whole, so the name it defines exists.
#[test]
fn a_call_in_a_block_runs_with_the_block() {
    app()
        .edit("app/models/article.rb", "  setting :title_limit, 40\n", "  %w[title_limit].each { |n| setting(n, 40) }\n")
        .run_ruby("raise Article.limit.inspect unless Article.limit == 40\nputs \"block passed\"\n")
        .assert_passes();
}

/// A module opened in two files keeps both openings in the Ruby emit, so the macro a reopening supplies is replayed and runs.
#[test]
fn a_reopened_module_s_macro_runs() {
    emit_and_run::real_blog()
        .write("lib/settings_extension.rb", "module SettingsExtension\n  def settings_loaded?\n    true\n  end\nend\n")
        .write("lib/settings_extension/setting.rb", "module SettingsExtension\n  def setting(name, default)\n    define_singleton_method(name) { default }\n  end\nend\n")
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true\n  extend SettingsExtension\n  setting :title_limit, 40\n\n  def self.limit\n    Article.title_limit\n  end\n",
        )
        .run_ruby("raise Article.limit.inspect unless Article.limit == 40\nraise \"loaded\" unless Article.settings_loaded?\nputs \"reopened passed\"\n")
        .assert_passes();
}

/// A condition or a loop the body runs whole (discourse's `if GlobalSetting.load_plugins?` over `Dir[…].each { |file| load_settings(file) }`) is replayed as written, its own locals with it.
#[test]
fn a_call_in_a_condition_or_loop_runs_with_the_whole_statement() {
    let overlay = || app().edit(
        "app/models/article.rb",
        "  setting :title_limit, 40\n",
        "  setting :title_limit, 40\n\n  if ENV[\"RH_NO_EXTRA_SETTINGS\"].nil?\n    extra = { \"body_limit\" => 500, \"slug_limit\" => 20 }\n    allowed = ->(name) { name != \"slug_limit\" }\n    extra.each do |name, value|\n      setting(name, value) if allowed.call(name)\n    end\n  end\n\n  def self.limits\n    [Article.title_limit, Article.body_limit]\n  end\n",
    );
    let errors = overlay().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
    overlay()
        .run_ruby("raise Article.limits.inspect unless Article.limits == [40, 500]\nraise \"slug\" if Article.respond_to?(:slug_limit)\nputs \"compound passed\"\n")
        .assert_passes();
}

/// A statement that also reads a local the body assigned before it is not carried: the replay does not stand beside that assignment.
#[test]
fn a_compound_statement_reading_an_outer_local_keeps_the_class_closed() {
    let errors = app()
        .edit("app/models/article.rb", "  setting :title_limit, 40\n", "  names = %w[title_limit]\n  if names.any?\n    names.each { |n| setting(n, 40) }\n  end\n")
        .check_errors();
    assert!(errors.iter().any(|e| e.contains("title_limit")), "{errors:#?}");
}

/// A statement that also runs a class-body call the replay does not carry (a framework macro) is left whole to the dropped-call report.
#[test]
fn a_compound_statement_with_another_body_call_keeps_the_class_closed() {
    let errors = app()
        .edit("app/models/article.rb", "  setting :title_limit, 40\n", "  if ENV[\"X\"].nil?\n    setting(:title_limit, 40)\n    validates :body, presence: true\n  end\n")
        .check_errors();
    assert!(errors.iter().any(|e| e.contains("title_limit")), "{errors:#?}");
}

/// The app states a computed name's type in `sig/`: the call reads that type, not untyped, and still runs.
#[test]
fn a_sig_declaration_types_a_computed_name() {
    let declared = || app().write("sig/article_settings.rbs", "class Article\n  def self.title_limit: () -> Integer\nend\n");
    let misuse = declared()
        .edit("app/models/article.rb", "  def self.limit\n", "  def self.shout\n    Article.title_limit.upcase\n  end\n\n  def self.limit\n")
        .check_errors();
    assert!(misuse.iter().any(|e| e.contains("upcase")), "{misuse:#?}");
    let errors = declared().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
    declared()
        .edit("app/models/article.rb", "  def self.limit\n", "  def self.next_limit\n    Article.title_limit + 1\n  end\n\n  def self.limit\n")
        .run_ruby("raise Article.next_limit.inspect unless Article.next_limit == 41\nputs \"sig passed\"\n")
        .assert_passes();
}

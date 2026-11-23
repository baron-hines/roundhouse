//! A class method an app writes to define methods (forem's
//! `Settings::Base.setting`, adapted from rails-settings-cached) is
//! expanded at each class-body call into the methods it defines. Its
//! arguments are captured as Ruby captures them: a literal is that
//! literal, any other value is evaluated once, where the call is.

use super::emit_and_run;

const BASE: &str = r###"class SettingsBase < ApplicationRecord
  self.abstract_class = true

  PROTECTED_KEYS = %w[var value].freeze

  class << self
    def setting(key, default: nil, type: :string)
      define_setting(key, default: default, type: type)
    end

    def keys
      @defined_settings.map { |s| s[:key] }
    end

    private

    def define_setting(key, default: nil, type: :string)
      key = key.to_s
      raise(ArgumentError, "Can't use '#{key}' as setting name") if key.in?(PROTECTED_KEYS)

      @defined_settings ||= []
      @defined_settings << { key: key, default: default, type: type }

      define_singleton_method(key) do |scope: nil|
        [1].each { |key| key.to_s }
        result = stored(key)
        result = default if result.nil?
        convert(type, result)
      end

      define_singleton_method(:"#{key}=") do |value|
        store(key, value)
        value
      end
    end

    def stored(key)
      (@values ||= {})[key]
    end

    def store(key, value)
      (@values ||= {})[key] = value.to_s
    end

    def convert(type, value)
      case type
      when :integer then value.to_i
      else value.to_s
      end
    end
  end
end
"###;

const SITE: &str = r#"class SiteConfig < SettingsBase
  self.table_name = :site_configs

  setting :community_name, type: :string, default: "Forem"
  setting :start_year, type: :integer, default: 2000 + 26
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/models/settings_base.rb", BASE)
        .write("app/models/site_config.rb", SITE)
        .edit(
            "db/schema.rb",
            "  add_foreign_key",
            "  create_table \"site_configs\", force: :cascade do |t|\n    t.string \"var\"\n    t.text \"value\"\n  end\n\n  add_foreign_key",
        )
}

const ASSERTIONS: &str = r##"got = [SiteConfig.community_name, SiteConfig.start_year.to_s]
SiteConfig.community_name = "Dev"
SiteConfig.start_year = 2030
got = got + [SiteConfig.community_name, SiteConfig.start_year.to_s] + SiteConfig.keys
raise got.inspect unless got == ["Forem", "2026", "Dev", "2030", "community_name", "start_year"]
puts "class macros passed"
"##;

#[test]
fn a_setting_macro_defines_its_methods() {
    app().run_ruby(ASSERTIONS).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_setting_macro_defines_its_methods_on_spinel() {
    app().run_spinel(ASSERTIONS).assert_passes();
}

/// A guard the call's arguments do not decide leaves the call as it was,
/// reported, rather than dropping the methods it would have defined.
#[test]
fn a_macro_it_cannot_run_is_reported() {
    let (_emitted, errors) = app()
        .edit("app/models/site_config.rb", "  setting :start_year", "  setting :nickname, type: (Time.now.year > 2000 ? :string : :integer)\n  setting :start_year")
        .edit("app/models/settings_base.rb", "      key = key.to_s\n", "      key = key.to_s\n      raise ArgumentError, \"bad type\" unless type == :string || type == :integer\n")
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("`setting` defines methods this call does not expand")), "{errors:#?}");
}

/// The expansion runs the macro's writes to the class's own state in the
/// calling class's body, which no strict target carries.
#[test]
fn a_class_body_state_write_is_unsupported_on_strict_targets() {
    let refused = std::panic::catch_unwind(|| { app().emit(roundhouse::project::BuildTarget::Rust); }).expect_err("the Rust emit refuses");
    let message = refused.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(message.contains("class-instance-variable initialization is not supported"), "{message}");
}

/// A rescue list with a splat (`rescue ArgumentError, *ERRORS => e`) in a
/// definition's block is not an argument splat: the IR keeps it as written
/// (a `Splat` among the rescue clause's classes), so the expansion keeps
/// the definition instead of refusing the macro (#1044).
#[test]
fn a_definition_rescuing_a_splatted_error_list_is_expanded() {
    let (_emitted, errors) = app()
        .edit(
            "app/models/settings_base.rb",
            "    def keys\n",
            "    def guarded(name)\n      define_singleton_method(name) do |hash, key|\n        begin\n          hash.fetch(key)\n        rescue ArgumentError, *LOOKUP_ERRORS => e\n          \"rescued #{e.class}\"\n        end\n      end\n    end\n\n    def keys\n",
        )
        .edit("app/models/settings_base.rb", "  PROTECTED_KEYS", "  LOOKUP_ERRORS = [KeyError, IndexError].freeze\n\n  PROTECTED_KEYS")
        .edit("app/models/site_config.rb", "  setting :community_name", "  guarded :lookup\n  setting :community_name")
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(!errors.iter().any(|e| e.contains("defines methods this call does not expand")), "{errors:#?}");
}

#[test]
fn a_definition_rescuing_a_splatted_error_list_runs() {
    app()
        .edit(
            "app/models/settings_base.rb",
            "    def keys\n",
            "    def guarded(name)\n      define_singleton_method(name) do |hash, key|\n        begin\n          hash.fetch(key)\n        rescue ArgumentError, *LOOKUP_ERRORS => e\n          \"rescued #{e.class}\"\n        end\n      end\n    end\n\n    def keys\n",
        )
        .edit("app/models/settings_base.rb", "  PROTECTED_KEYS", "  LOOKUP_ERRORS = [KeyError, IndexError].freeze\n\n  PROTECTED_KEYS")
        .edit("app/models/site_config.rb", "  setting :community_name", "  guarded :lookup\n  setting :community_name")
        .run_ruby(
            "got = [SiteConfig.lookup({ \"a\" => 1 }, \"a\").to_s, SiteConfig.lookup({}, \"b\")]\nraise got.inspect unless got == [\"1\", \"rescued KeyError\"]\nputs \"splat rescue passed\"\n",
        )
        .assert_passes();
}

/// Control: an argument splat in the same definition is still one the
/// expansion would erase, so the macro is still refused.
#[test]
fn a_definition_with_an_argument_splat_is_still_refused() {
    let (_emitted, errors) = app()
        .edit(
            "app/models/settings_base.rb",
            "    def keys\n",
            "    def guarded(name)\n      define_singleton_method(name) do |hash, key|\n        begin\n          hash.fetch(*[key])\n        rescue ArgumentError, *LOOKUP_ERRORS => e\n          \"rescued #{e.class}\"\n        end\n      end\n    end\n\n    def keys\n",
        )
        .edit("app/models/settings_base.rb", "  PROTECTED_KEYS", "  LOOKUP_ERRORS = [KeyError, IndexError].freeze\n\n  PROTECTED_KEYS")
        .edit("app/models/site_config.rb", "  setting :community_name", "  guarded :lookup\n  setting :community_name")
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("`guarded` defines methods this call does not expand")), "{errors:#?}");
}

/// A macro reads a list its class inherits (`NAMES` on the root, the loop in the subclass), and a subclass that
/// shadows the name does not change what the inherited macro reads, as Ruby's lexical-then-ancestor lookup.
#[test]
fn an_inherited_list_constant_folds_from_where_the_macro_is_written() {
    let run = emit_and_run::real_blog()
        .write(
            "app/models/macro_root.rb",
            "class MacroRoot\n  NAMES = %i[alpha beta].freeze\nend\n",
        )
        .write(
            "app/models/macro_leaf.rb",
            "class MacroLeaf < MacroRoot\n  def self.install\n    NAMES.each do |n|\n      define_method(\"#{n}_name\") { n.to_s }\n    end\n  end\nend\n",
        )
        .write("app/models/macro_thing.rb", "class MacroThing < MacroLeaf\n  NAMES = %i[gamma].freeze\n  install\nend\n")
        .run_ruby(
            "t = MacroThing.new\ngot = [t.alpha_name, t.beta_name, t.respond_to?(:gamma_name)]\nraise \"got #{got.inspect}\" unless got == [\"alpha\", \"beta\", false]\nputs \"inherited constant passed\"\n",
        );
    run.assert_passes();
    assert!(run.stdout.contains("inherited constant passed"), "{}", run.stdout);
}

/// The class the macro is written in assigns the name a value that does not fold: that value hides the root's, so the
/// root's list is not read in its place and the macro is not expanded from it.
#[test]
fn a_nearer_unfoldable_list_hides_the_inherited_one() {
    let (_emitted, errors) = emit_and_run::real_blog()
        .write("app/models/macro_root.rb", "class MacroRoot\n  NAMES = %i[alpha beta].freeze\nend\n")
        .write(
            "app/models/macro_leaf.rb",
            "class MacroLeaf < MacroRoot\n  NAMES = ENV.fetch(\"NAMES\", \"gamma\").split(\",\").map(&:to_sym)\n\n  def self.install\n    NAMES.each do |n|\n      define_method(\"#{n}_name\") { n.to_s }\n    end\n  end\nend\n",
        )
        .write("app/models/macro_thing.rb", "class MacroThing < MacroLeaf\n  install\nend\n")
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("`install` defines methods this call does not expand")), "{errors:#?}");
}

/// A macro's default (`def self.install(names = NAMES)`) is read where the method is written, as Ruby evaluates it:
/// the root's list, though a subclass between assigns the name too.
#[test]
fn a_default_reads_its_methods_own_constant() {
    let run = emit_and_run::real_blog()
        .write(
            "app/models/macro_root.rb",
            "class MacroRoot\n  NAMES = %i[alpha].freeze\n\n  def self.install(names = NAMES)\n    names.each do |n|\n      define_method(\"#{n}_name\") { n.to_s }\n    end\n  end\nend\n",
        )
        .write("app/models/macro_leaf.rb", "class MacroLeaf < MacroRoot\n  NAMES = %i[beta].freeze\nend\n")
        .write("app/models/macro_thing.rb", "class MacroThing < MacroLeaf\n  install\nend\n")
        .run_ruby("t = MacroThing.new\ngot = [t.alpha_name, t.respond_to?(:beta_name)]\nraise \"got #{got.inspect}\" unless got == [\"alpha\", false]\nputs \"default constant passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("default constant passed"), "{}", run.stdout);
}

/// discourse's `DiscoursePluginRegistry.define_register`: a plain class's
/// own macro guards on `respond_to?` of the name it defines, records it in
/// a class variable, aliases one singleton method under another name and
/// yields itself when given a block. The guard answers as the class body
/// sees it at that call, the record runs in the class body, and the alias
/// is a copy of the method as the run defined it.
const REGISTRY: &str = r##"class PluginRegistry
  @@register_names = Set.new

  def self.define_register(register_name, type)
    return if respond_to?(register_name)
    @@register_names << register_name

    define_singleton_method(register_name) do
      instance_variable_get(:"@#{register_name}") ||
        instance_variable_set(:"@#{register_name}", type.new)
    end

    define_method(register_name) { self.class.public_send(register_name) }
  end

  def self.define_filtered_register(register_name)
    return if respond_to?(register_name)
    define_register(register_name, Array)

    singleton_class.alias_method :"_raw_#{register_name}", :"#{register_name}"

    define_singleton_method(register_name) do
      public_send(:"_raw_#{register_name}").filter_map { |h| h[:value] if h[:enabled] }.uniq
    end

    define_singleton_method("register_#{register_name.to_s.singularize}") do |value, enabled|
      public_send(:"_raw_#{register_name}") << { enabled: enabled, value: value }
    end

    yield(self) if block_given?
  end

  define_register :javascripts, Set
  define_register :javascripts, Hash
  define_filtered_register :stylesheets
end
"##;

const REGISTRY_ASSERTIONS: &str = r##"PluginRegistry.javascripts << "a.js"
PluginRegistry.register_stylesheet("x.css", true)
PluginRegistry.register_stylesheet("y.css", false)
PluginRegistry.register_stylesheet("x.css", true)
got = [PluginRegistry.javascripts.to_a, PluginRegistry.new.javascripts.class.name, PluginRegistry.stylesheets, PluginRegistry._raw_stylesheets.size, PluginRegistry.class_variable_get(:@@register_names).to_a.map(&:to_s)]
raise got.inspect unless got == [["a.js"], "Set", ["x.css"], 3, ["javascripts", "stylesheets"]]
puts "registry macros passed"
"##;

fn registry_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/plugin_registry.rb", REGISTRY)
}

#[test]
fn a_registry_macro_guarded_on_respond_to_defines_its_methods() {
    let (_emitted, errors) = registry_app().emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    registry_app().run_ruby(REGISTRY_ASSERTIONS).assert_passes();
}

/// A class that `extend`s a module the program does not define may answer
/// any name on its class side: the guard is not answered, and the call is
/// reported rather than expanded past a method the module may provide.
#[test]
fn a_registry_macro_beside_an_unknown_extend_is_reported() {
    let errors = emit_and_run::real_blog()
        .write("app/models/plugin_registry.rb", &REGISTRY.replacen("class PluginRegistry\n", "class PluginRegistry\n  extend Forwardable\n", 1))
        .check_errors();
    assert!(errors.iter().any(|e| e.contains("`define_register` defines methods this call does not expand")), "{errors:#?}");
}

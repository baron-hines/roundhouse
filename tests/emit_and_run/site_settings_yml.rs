//! A settings class whose methods `config/site_settings.yml` defines as it loads (discourse's `SiteSetting` shape):
//! each entry read through `setup_methods`, the `define_singleton_method` its type reaches written as a method.

#[path = "../support/scratch.rs"]
mod scratch;
use super::emit_and_run;

const YML: &str = r#"basic:
  title:
    default: "Forum"
    client: true
  max_posts: 5
  allow_guests: yes
  ratio: 1.5
  word: hello
  tags:
    type: list
    default: "a|b"
  level:
    enum: "LevelSetting"
    default: "low"
  wrapped_level:
    enum: "RankSetting"
    default: "high"
"#;

const LOADER: &str = r#"class SettingsFile
  def initialize(file)
    @file = file
  end

  def load
    YAML.load_file(@file).each do |category, settings|
      settings.each do |setting_name, hash|
        if hash.is_a?(Hash)
          value = hash.delete("default")
          yield category, setting_name, value, hash.transform_keys(&:to_sym)
        else
          yield category, setting_name, hash, {}
        end
      end
    end
  end
end
"#;

const EXTENSION: &str = r##"module SiteSettingExtension
  def current
    @current ||= {}
  end

  def static_types
    @static_types ||= {}
  end

  def enums
    @enums ||= {}
  end

  def type_supervisor
    self
  end

  def get_type(name)
    static_types[name]
  end

  def get_enum_class(name)
    enums[name]
  end

  def get_list_type(name)
    nil
  end

  def load_settings(file)
    SettingsFile.new(file).load do |category, name, default, opts|
      setting(name, default, opts.merge(category: category))
    end
  end

  def setting(name_arg, default = nil, opts = {})
    name = name_arg.to_sym
    current[name] = default
    enums[name] = Object.const_get(opts[:enum]) if opts[:enum]
    static_types[name] = if opts[:type]
      opts[:type].to_sym
    elsif opts[:enum]
      :enum
    elsif default == true || default == false
      :bool
    elsif default.is_a?(Integer)
      :integer
    elsif default.is_a?(Float)
      :float
    else
      :string
    end
    setup_methods(name)
  end

  def add_override!(name, val)
    current[name] = val
  end

  def setup_methods(name)
    clean_name = name.to_s.sub("?", "").to_sym

    enum_wrapper =
      if type_supervisor.get_type(name) == :enum
        klass = type_supervisor.get_enum_class(name)
        klass if klass.respond_to?(:wrap)
      end

    define_singleton_method clean_name do |scoped_to = nil|
      value = current[name]
      return enum_wrapper.wrap(value.to_s) if enum_wrapper
      value
    end

    if %i[list emoji_list].include?(type_supervisor.get_type(name))
      define_singleton_method("#{clean_name}_map") do |scoped_to = nil|
        public_send(clean_name, scoped_to).to_s.split("|")
      end
    end

    define_singleton_method "#{clean_name}?" do |scoped_to = nil|
      public_send(clean_name, scoped_to)
    end

    define_singleton_method "#{clean_name}=" do |val|
      add_override!(name, val)
    end
  end
end
"##;

const ENUMS: &str = r##"class LevelSetting
  def self.values
    %w[low high]
  end
end

class RankSetting
  def self.wrap(value)
    "<#{value}>"
  end
end
"##;

const MODEL: &str = r#"class SiteSetting < ApplicationRecord
  extend SiteSettingExtension

  load_settings(Rails.root.join("config/site_settings.yml").to_s)
end
"#;

const READS: &str = r#"class SettingsReport
  def self.lines
    [
      SiteSetting.title,
      SiteSetting.max_posts + 1,
      SiteSetting.allow_guests?,
      SiteSetting.ratio,
      SiteSetting.word,
      SiteSetting.tags_map.join(","),
      SiteSetting.level,
      SiteSetting.wrapped_level,
    ]
  end

  def self.renamed
    SiteSetting.title = "Board"
    SiteSetting.title
  end
end
"#;

fn overlay() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("db/schema.rb", "create_table \"articles\", force: :cascade do |t|", "create_table \"site_settings\", force: :cascade do |t|\n    t.string \"name\"\n  end\n\n  create_table \"articles\", force: :cascade do |t|")
        .write("config/site_settings.yml", YML)
        .write("lib/settings_file.rb", LOADER)
        .write("lib/site_setting_extension.rb", EXTENSION)
        .write("app/models/level_setting.rb", ENUMS)
        .write("app/models/site_setting.rb", MODEL)
        .write("app/models/settings_report.rb", READS)
}

#[test]
fn a_setting_the_yml_defines_reads_as_the_loader_defines_it() {
    let errors = overlay().check_errors();
    assert!(!errors.iter().any(|e| e.contains("on SiteSetting")), "{errors:#?}");
    let run = overlay().run_ruby(
        "got = SettingsReport.lines\nexpected = [\"Forum\", 6, true, 1.5, \"hello\", \"a,b\", \"low\", \"<high>\"]\nraise got.inspect unless got == expected\nraise SettingsReport.renamed unless SettingsReport.renamed == \"Board\"\nputs \"settings passed\"\n",
    );
    run.assert_passes();
    assert!(run.stdout.contains("settings passed"));
}

/// The methods each entry's type reaches, and none for one it does not: `tags` is a list (`tags_map`), `title` is not.
#[test]
fn each_setting_gets_the_methods_its_type_reaches() {
    let root = scratch::dir("roundhouse-site-settings-yml");
    let source = root.join("app");
    std::fs::create_dir_all(&root).unwrap();
    assert!(std::process::Command::new("cp").args(["-a"]).arg(roundhouse::fixtures::real_blog()).arg(&source).status().unwrap().success());
    for (path, text) in [
        ("config/site_settings.yml", YML),
        ("lib/settings_file.rb", LOADER),
        ("lib/site_setting_extension.rb", EXTENSION),
        ("app/models/level_setting.rb", ENUMS),
        ("app/models/site_setting.rb", MODEL),
        ("app/models/settings_report.rb", READS),
    ] {
        std::fs::write(source.join(path), text).unwrap();
    }
    let schema = std::fs::read_to_string(source.join("db/schema.rb")).unwrap().replacen(
        "create_table \"articles\", force: :cascade do |t|",
        "create_table \"site_settings\", force: :cascade do |t|\n    t.string \"name\"\n  end\n\n  create_table \"articles\", force: :cascade do |t|",
        1,
    );
    std::fs::write(source.join("db/schema.rb"), schema).unwrap();
    let app = roundhouse::ingest::ingest_app(&source).expect("ingest");
    std::fs::remove_dir_all(&root).ok();
    let model = app.models.iter().find(|m| m.name.0.as_str() == "SiteSetting").expect("SiteSetting");
    let names: Vec<&str> = model.methods().filter(|m| m.receiver == roundhouse::dialect::MethodReceiver::Class).map(|m| m.name.as_str()).collect();
    for name in ["title", "title?", "title=", "max_posts", "allow_guests?", "tags_map", "level"] {
        assert!(names.contains(&name), "{name}: {names:?}");
    }
    assert!(!names.contains(&"title_map"), "{names:?}");
}

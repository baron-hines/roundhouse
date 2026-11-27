//! `const_get` with a literal name on a class object, in a source gem: the
//! class or module it names (devise's `Devise::Models.const_get(...)`, then
//! that module's `const_get("ClassMethods")`), whether chained or through
//! a local. `Object`'s are the top level's. And `self::VERSION` in a
//! module only `extend`ed, whose self is the class or module extending it.

use super::emit_and_run;

const GEM_DIR: &str = "vendor/bundle/ruby/3.4.0/gems/plugs-1.0.0";

const PLUGS: &str = r#"module Plugs
  def self.kind = "plugs"

  module Auth
    def self.kind = "auth"
  end

  # version_gem's shape: a module only `extend`ed, whose methods read the extender's constants.
  module Versioned
    def label = "v#{self::VERSION}"

    def kind_of_version = const_get(:VERSION).split(".").first
  end

  module Release
    VERSION = "2.5.0"
    extend Versioned
  end

  def self.chained = Object.const_get("Plugs").const_get("Auth").kind

  def self.through_local
    plugs = Object.const_get(:Plugs)
    [plugs.kind, plugs.const_get(:Auth).kind]
  end
end
"#;

const PROBE: &str = r#"class PlugsProbe
  def self.describe = [Plugs.chained, *Plugs.through_local, Plugs::Release.label, Plugs::Release.kind_of_version]
end
"#;

fn plugs_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    debug (1.11.1)\n    plugs (1.0.0)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  plugs\n")
        .edit("Gemfile", "gem \"rails\", \"~> 8.1.4\"\n", "gem \"rails\", \"~> 8.1.4\"\ngem \"plugs\"\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write(&format!("{GEM_DIR}/lib/plugs.rb"), PLUGS)
        .write("roundhouse.yml", "extensions:\n  - config/roundhouse\n")
        .write("config/roundhouse/gems.yml", "source_gems:\n  - {gem: \"plugs\"}\n")
        .write("app/models/plugs_probe.rb", PROBE)
}

const ASSERTIONS: &str = r##"got = PlugsProbe.describe
raise got.inspect unless got == ["auth", "plugs", "auth", "v2.5.0", "2"]
puts "module reflection passed"
"##;

#[test]
fn literal_const_get_runs() {
    let run = plugs_app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("module reflection passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn literal_const_get_runs_on_spinel() {
    let run = plugs_app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("module reflection passed"), "{}", run.stdout);
}

/// A module some class also `include`s runs its methods with an instance as self there, so its
/// `self::VERSION` stays refused.
#[test]
fn an_included_modules_self_constant_is_still_refused() {
    let app = plugs_app().edit(
        &format!("{GEM_DIR}/lib/plugs.rb"),
        "  def self.chained",
        "  class Holder\n    include Versioned\n  end\n\n  def self.chained",
    );
    let (_emitted, errors) = app.emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.iter().any(|e| e.contains("const_get requires a proven class/module object receiver")), "{errors:#?}");
}

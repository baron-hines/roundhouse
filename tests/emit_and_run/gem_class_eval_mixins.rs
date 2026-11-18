//! version_gem's pinning, `Foo::Version.class_eval do extend VersionGem::Basic end`
//! at a gem file's top level: the block's constants are the top level's, and
//! its mixins run on the receiver as `Foo::Version.extend(...)` would.

use super::emit_and_run;

const GEM_DIR: &str = "vendor/bundle/ruby/3.4.0/gems/pinned-1.0.0";

const PINNED: &str = r#"module Pinned
  module Basic
    def to_s = self::VERSION

    def major = self::VERSION.split(".").first
  end

  module Version
    VERSION = "3.4.5"
  end
end

Pinned::Version.class_eval do
  extend Pinned::Basic
end
"#;

const PROBE: &str = r#"class PinnedProbe
  def self.describe = [Pinned::Version.to_s, Pinned::Version.major]
end
"#;

fn pinned_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    debug (1.11.1)\n    pinned (1.0.0)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  pinned\n")
        .edit("Gemfile", "gem \"rails\", \"~> 8.1.4\"\n", "gem \"rails\", \"~> 8.1.4\"\ngem \"pinned\"\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write(&format!("{GEM_DIR}/lib/pinned.rb"), PINNED)
        .write("roundhouse.yml", "extensions:\n  - config/roundhouse\n")
        .write("config/roundhouse/gems.yml", "source_gems:\n  - {gem: \"pinned\"}\n")
        .write("app/models/pinned_probe.rb", PROBE)
}

const ASSERTIONS: &str = r##"got = PinnedProbe.describe
raise got.inspect unless got == ["3.4.5", "3"]
puts "class_eval mixins passed"
"##;

#[test]
fn a_file_level_class_eval_mixin_runs() {
    let run = pinned_app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("class_eval mixins passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_file_level_class_eval_mixin_runs_on_spinel() {
    let run = pinned_app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("class_eval mixins passed"), "{}", run.stdout);
}

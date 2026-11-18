//! version_gem's pinning, `Foo::Version.class_eval do extend VersionGem::Basic end`
//! at a gem file's top level: the block's constants are the top level's, and
//! its mixins run on the receiver as `Foo::Version.extend(...)` would, so the
//! module's `self::VERSION` is the receiver's.
//!
//! version_gem's own Basic goes one step further: its `extended` hook, written
//! in `class << self`, guards with `raise … unless defined?(base::VERSION)` and
//! extends the receiver with Api. Spinel runs it since matz/spinel#12113
//! (the file-level `extend`'s hook) and #12114 (`defined?(base::VERSION)`).

use super::emit_and_run;

const GEM_DIR: &str = "vendor/bundle/ruby/3.4.0/gems/pinned-1.0.0";

/// The module the block extends with reads the receiver's constant itself.
const PLAIN: &str = r#"module Pinned
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

/// version_gem's Basic: its hook extends the receiver with Api.
const HOOKED: &str = r#"module Pinned
  module Api
    def to_s = self::VERSION

    def major = self::VERSION.split(".").first
  end

  module Basic
    class << self
      def extended(base)
        raise ArgumentError, "VERSION must be defined before 'extend #{name}'" unless defined?(base::VERSION)

        base.extend(Api)
      end
    end
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

fn pinned_app(gem: &str) -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    debug (1.11.1)\n    pinned (1.0.0)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  pinned\n")
        .edit("Gemfile", "gem \"rails\", \"~> 8.1.4\"\n", "gem \"rails\", \"~> 8.1.4\"\ngem \"pinned\"\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write(&format!("{GEM_DIR}/lib/pinned.rb"), gem)
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
    let run = pinned_app(PLAIN).run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("class_eval mixins passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_file_level_class_eval_mixin_runs_on_spinel() {
    let run = pinned_app(PLAIN).run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("class_eval mixins passed"), "{}", run.stdout);
}

#[test]
fn an_extended_hook_the_mixin_fires_runs() {
    let run = pinned_app(HOOKED).run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("class_eval mixins passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_extended_hook_the_mixin_fires_runs_on_spinel() {
    let run = pinned_app(HOOKED).run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("class_eval mixins passed"), "{}", run.stdout);
}

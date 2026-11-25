//! A source gem whose Zeitwerk loader manages its files (mission_control-jobs' `loader.push_dir`): production Rails
//! eager-loads every managed file at boot, so a file nothing names still runs (`Relay.loaded << :registry`), and an
//! ignored one does not. What it reads loads as MC's does: a class under `I18n::Config`, one under
//! `Rails::Command::Base`, and a `mattr_accessor` defaulting to `ActiveSupport::Logger.new(nil)`.

use super::emit_and_run;

const GEM: &str = "vendor/bundle/ruby/3.4.0/gems/relay-1.0";

const ENTRY: &str = r##"require "zeitwerk"

loader = Zeitwerk::Loader.new
loader.push_dir(__dir__)
loader.ignore("#{__dir__}/relay/tasks.rb")
loader.setup

module Relay
  mattr_accessor :logger, default: ActiveSupport::Logger.new(nil)
  mattr_accessor :loaded, default: []
end
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    debug (1.11.1)\n    relay (1.0)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  relay\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write("roundhouse.yml", "extensions:\n  - config/roundhouse\n")
        .write("config/roundhouse/gems.yml", "source_gems:\n  - {gem: \"relay\"}\n")
        .write(&format!("{GEM}/lib/relay.rb"), ENTRY)
        .write(&format!("{GEM}/lib/relay/registry.rb"), "module Relay\n  class Registry\n  end\nend\n\nRelay.loaded << :registry\nRelay.loaded << Relay::LocaleConfig.superclass.name\nRelay.loaded << Relay::Authentication.superclass.name\n")
        .write(&format!("{GEM}/lib/relay/tasks.rb"), "Relay.loaded << :tasks\n")
        .write(&format!("{GEM}/lib/relay/locale_config.rb"), "module Relay\n  class LocaleConfig < I18n::Config\n  end\nend\n")
        .write(&format!("{GEM}/lib/relay/authentication.rb"), "module Relay\n  class Authentication < Rails::Command::Base\n  end\nend\n")
}

#[test]
fn a_zeitwerk_managed_gem_file_loads_at_boot() {
    let run = app().run_ruby(
        "raise Relay.loaded.inspect unless Relay.loaded == [:registry, \"I18n::Config\", \"Rails::Command::Base\"]\n\
         raise \"logger\" unless Relay.logger.is_a?(ActiveSupport::Logger)\n\
         puts \"zeitwerk eager gem passed\"\n",
    );
    run.assert_passes();
    assert!(run.stdout.contains("zeitwerk eager gem passed"), "{}", run.stdout);
}

//! The registry `tests/emit_and_run/class_param_or_assign.rs` and its Spinel twin in
//! `tests/spinel_toolchain/class_param_or_assign.rs` share.

use super::emit_and_run;

pub const REGISTRY: &str = r#"class Registry
  module Runnable
  end

  class Base
    include Runnable

    def run
      "base"
    end
  end

  class Plain < Base
    def run
      "plain"
    end
  end

  class Fancy < Base
    def run
      "fancy"
    end
  end

  @@kinds = {}

  def self.add(label, kind = nil)
    kind ||= Class.new(Registry::Base)
    raise "no run" unless kind.method_defined?(:run)
    raise "not a base" unless kind.ancestors.include?(Registry::Base)
    raise "not runnable" unless kind.include?(Registry::Runnable)
    @@kinds[label] = kind
  end

  def self.run(label)
    @@kinds[label].new.run
  end
end
"#;

pub const SCRIPT: &str = r#"got = [Registry.run(:plain), Registry.run(:fancy)]
raise "got #{got.inspect}" unless got == ["plain", "fancy"]
puts "class parameter passed"
"#;

pub fn app() -> emit_and_run::Overlay {
    let gem = "vendor/bundle/ruby/3.4.0/gems/kinds-1.0";
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    debug (1.11.1)\n    kinds (1.0)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  kinds\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write("roundhouse.yml", "extensions:\n  - config/roundhouse\n")
        .write("config/roundhouse/gems.yml", "source_gems:\n  - {gem: \"kinds\"}\n")
        .write(&format!("{gem}/lib/kinds.rb"), "require \"kinds/registry\"\n")
        .write(&format!("{gem}/lib/kinds/registry.rb"), REGISTRY)
        .write("config/initializers/registry.rb", "Registry.add(:plain, Registry::Plain)\nRegistry.add(:fancy, Registry::Fancy)\n")
}

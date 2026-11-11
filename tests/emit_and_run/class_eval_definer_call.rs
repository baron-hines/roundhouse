//! devise's `Devise::Models.config(self, :pepper, :stretches)`: a module's method handed the class
//! body's `self` defines on it with `class << mod; attr_accessor …; end` and `class_eval` strings.
//! Ruby runs it as the body loads; the methods it defines are the class's own, in `module
//! ClassMethods` the includer's class methods. `Tunable` and `Plain` hold nothing but the call.

use super::emit_and_run;

const KIT: &str = r##"module SettingsKit
  def self.config(mod, *accessors)
    class << mod; attr_accessor :available_configs; end
    mod.available_configs = accessors

    accessors.each do |accessor|
      mod.class_eval <<-METHOD, __FILE__, __LINE__ + 1
        def #{accessor}
          if defined?(@#{accessor})
            @#{accessor}
          else
            "default #{accessor}"
          end
        end

        def #{accessor}=(value)
          @#{accessor} = value
        end
      METHOD
    end
  end
end
"##;

const TUNABLE: &str = r#"module Tunable
  extend ActiveSupport::Concern

  module ClassMethods
    SettingsKit.config(self, :pepper, :stretches)
  end
end
"#;

const PLAIN: &str = r#"module Plain
  SettingsKit.config(self, :mode)
end
"#;

const GAUGE: &str = r#"class Gauge
  include Plain

  def tune!
    self.mode = "fast"
  end
end
"#;

const SCRIPT: &str = r#"raise "pepper" unless Article.pepper == "default pepper"
Article.season!
raise "set" unless Article.pepper == "salt" && Article.stretches == "default stretches"
Article.stretch!
raise "stretched" unless Article.stretches == "ten"
raise "configs" unless Plain.available_configs == [:mode]
gauge = Gauge.new
raise "mode" unless gauge.mode == "default mode"
gauge.tune!
raise "mode set" unless gauge.mode == "fast"
puts "load-time definer passed"
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("lib/settings_kit.rb", KIT)
        .write("app/models/concerns/tunable.rb", TUNABLE)
        .write("lib/plain.rb", PLAIN)
        .write("lib/gauge.rb", GAUGE)
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  include Tunable\n\n  def self.season!\n    self.pepper = \"salt\"\n  end\n\n  def self.stretch!\n    self.stretches = \"ten\"\n  end\n")
}

#[test]
fn a_load_time_definer_defines_on_the_class_it_is_handed() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("load-time definer passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_load_time_definer_defines_on_the_class_it_is_handed_natively() {
    let run = app().run_spinel(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("load-time definer passed"), "{}", run.stdout);
}

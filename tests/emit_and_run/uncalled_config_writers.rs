//! An ivar nothing writes reads as nil, so `defined?(@x)` is false: Devise's `Models.config` readers fall
//! back to the module's default when nothing calls the writer the same call defines. In a gem compiled from
//! source the Spinel tree leaves out the writer nothing calls, and its copy on the model goes with it
//! (`source_gem_shake`), so the reader is the only one left to type the ivar.

use super::emit_and_run;

const KIT: &str = r##"module SettingsKit
  def self.config(mod, *accessors)
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

const TUNABLE: &str = r#"module SettingsKit
  module Tunable
    extend ActiveSupport::Concern

    module ClassMethods
      SettingsKit.config(self, :pepper)
    end
  end
end
"#;

const FLAG_SCRIPT: &str = r#"raise "flag #{Article.flag.inspect}" unless Article.flag == "none"
puts "unwritten ivar passed"
"#;

const SCRIPT: &str = r#"raise "flag #{Article.flag.inspect}" unless Article.flag == "none"
raise "pepper #{Article.seasoning.inspect}" unless Article.seasoning == "default pepper"
puts "uncalled config writers passed"
"#;

const FLAG: &str = "  def self.flag\n    if defined?(@flag)\n      @flag\n    else\n      \"none\"\n    end\n  end\n\n";

fn installed() -> emit_and_run::Overlay {
    let gem = "vendor/bundle/ruby/3.4.0/gems/settings_kit-1.0";
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    debug (1.11.1)\n    settings_kit (1.0)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  settings_kit\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write(&format!("{gem}/lib/settings_kit.rb"), "require \"settings_kit/kit\"\nrequire \"settings_kit/tunable\"\n")
        .write(&format!("{gem}/lib/settings_kit/kit.rb"), KIT)
        .write(&format!("{gem}/lib/settings_kit/tunable.rb"), TUNABLE)
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", &format!("class Article < ApplicationRecord\n  include SettingsKit::Tunable\n\n  def self.seasoning\n    pepper\n  end\n\n{FLAG}"))
}

#[test]
fn an_unwritten_class_ivar_reads_nil() {
    let app = || emit_and_run::real_blog().edit("app/models/article.rb", "class Article < ApplicationRecord\n", &format!("class Article < ApplicationRecord\n{FLAG}"));
    let run = app().run_ruby(FLAG_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("unwritten ivar passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_unwritten_class_ivar_reads_nil_natively() {
    let run = emit_and_run::real_blog()
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", &format!("class Article < ApplicationRecord\n{FLAG}"))
        .run_spinel(FLAG_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("unwritten ivar passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_uncalled_config_writer_leaves_the_spinel_tree_with_its_copy() {
    let run = installed().run_spinel(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("uncalled config writers passed"), "{}", run.stdout);
}

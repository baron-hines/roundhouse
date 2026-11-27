//! A gem compiled from source branches at load time on the Ruby that runs it (`if RUBY_ENGINE != 'jruby'`,
//! `def … end if RUBY_VERSION < '3.3'`, `if defined?(JRUBY_VERSION)`): every Ruby the trees run on (4.0 or later,
//! CRuby's and Spinel's) takes the same arm, which the ingest reads as plain declarations. The visibility change and
//! the constant in a branch refused the whole file, which left the gem's classes out of the tree.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;

const GEM: &str = "vendor/bundle/ruby/3.4.0/gems/rubyish-0.1.0";

const LOCK: &str = "GEM\n  remote: https://rubygems.org/\n  specs:\n    rubyish (0.1.0)\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n  rubyish\n";

const ENTRY: &str = r#"module Rubyish
  class Shape
    def name = :shape

    def legacy = :old if RUBY_VERSION < "3.3"

    if RUBY_ENGINE != "jruby"
      alias_method :hidden_name, :name
      private :hidden_name
    end

    if defined?(JRUBY_VERSION)
      KIND = :java
    else
      KIND = :c
    end

    def report = [hidden_name, KIND, respond_to?(:legacy), respond_to?(:hidden_name)]
  end
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("Gemfile", "source \"https://rubygems.org\"\ngem \"rubyish\"\n")
        .write("Gemfile.lock", LOCK)
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write("roundhouse.yml", "extensions:\n  - config/roundhouse\n")
        .write("config/roundhouse/gems.yml", "source_gems:\n  - {gem: \"rubyish\"}\n")
        .write("config/application.rb", "Bundler.require(*Rails.groups)\n")
        .write(&format!("{GEM}/rubyish.gemspec"), "Gem::Specification.new do |s|\n  s.name = \"rubyish\"\n  s.version = \"0.1.0\"\n  s.summary = \"x\"\n  s.authors = [\"roundhouse\"]\n  s.files = Dir[\"lib/**/*.rb\"]\nend\n")
        .write(&format!("{GEM}/lib/rubyish.rb"), ENTRY)
        .write("app/models/ruby_report.rb", "module RubyReport\n  def self.lines = Rubyish::Shape.new.report\nend\n")
}

#[test]
fn a_branch_every_ruby_the_trees_run_on_takes_alike_is_read_as_its_arm() {
    let (dir, errors) = app().emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let out = emit_and_run::ruby()
        .current_dir(&*dir)
        .args(["-e", "require_relative 'boot'\np RubyReport.lines\n"])
        .output()
        .expect("ruby");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert_eq!(stdout, "[:shape, :c, false, false]\n", "{stderr}");
}

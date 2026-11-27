//! A gem compiled from source aliases a method Ruby finds outside the class body: Object's (`alias original_dup dup`
//! in a class that names no superclass, ast's), an included core module's (`alias index find_index` beside
//! `include Enumerable`, capybara's), an app module the class includes by its short name from a file of its own
//! (`include Node`, rexml's), a core class the gem reopens (`class String`), and a class method in `class << self`
//! the body defined above it (`alias default_imap_port default_port`, net-imap's). Each refused its whole file, which
//! left the gem's classes out of the tree; each now loads and answers as Ruby 4.0 does.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;

const GEM: &str = "vendor/bundle/ruby/3.4.0/gems/aliasy-0.1.0";

const LOCK: &str = "GEM\n  remote: https://rubygems.org/\n  specs:\n    aliasy (0.1.0)\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n  aliasy\n";

const ENTRY: &str = r#"require_relative "aliasy/node"

module Aliasy
  class Plain
    alias original_dup dup
  end

  class Pair
    include Enumerable

    def each
      yield :a
      yield :b
    end

    alias index find_index
  end

  class Child
    include Node

    alias next_sibling next_sibling_node
  end

  class Ports
    def self.default_port = 143

    class << self
      alias default_imap_port default_port
    end
  end
end

class String
  alias aliasy_shout upcase
end
"#;

const NODE: &str = "module Aliasy\n  module Node\n    def next_sibling_node = :sibling\n  end\nend\n";

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("Gemfile", "source \"https://rubygems.org\"\ngem \"aliasy\"\n")
        .write("Gemfile.lock", LOCK)
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write("roundhouse.yml", "extensions:\n  - config/roundhouse\n")
        .write("config/roundhouse/gems.yml", "source_gems:\n  - {gem: \"aliasy\"}\n")
        .write("config/application.rb", "Bundler.require(*Rails.groups)\n")
        .write(&format!("{GEM}/aliasy.gemspec"), "Gem::Specification.new do |s|\n  s.name = \"aliasy\"\n  s.version = \"0.1.0\"\n  s.summary = \"x\"\n  s.authors = [\"roundhouse\"]\n  s.files = Dir[\"lib/**/*.rb\"]\nend\n")
        .write(&format!("{GEM}/lib/aliasy.rb"), ENTRY)
        .write(&format!("{GEM}/lib/aliasy/node.rb"), NODE)
        .write(
            "app/models/alias_report.rb",
            "module AliasReport\n  def self.lines = [Aliasy::Plain.new.original_dup.class.name, Aliasy::Pair.new.index(:b), Aliasy::Child.new.next_sibling, Aliasy::Ports.default_imap_port, \"hi\".aliasy_shout]\nend\n",
        )
}

#[test]
fn an_alias_ruby_resolves_outside_the_body_loads_the_gems_file() {
    let (dir, errors) = app().emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let out = emit_and_run::ruby()
        .current_dir(&*dir)
        .args(["-e", "require_relative 'boot'\np AliasReport.lines\n"])
        .output()
        .expect("ruby");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert_eq!(stdout, "[\"Aliasy::Plain\", 1, :sibling, 143, \"HI\"]\n", "{stderr}");
}

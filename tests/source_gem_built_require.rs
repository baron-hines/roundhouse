//! A gem compiled from source that requires one of its own files by a path it builds from `__FILE__`
//! (aasm's `require_persistence`): the tree carries each of the gem's files at its place under the mount, where
//! `__FILE__` names it and the gem's data already is, so the built path finds the file the gem shipped.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;

const GEM: &str = "vendor/bundle/ruby/3.4.0/gems/pathy-0.1.0";

const LOCK: &str = "GEM\n  remote: https://rubygems.org/\n  specs:\n    pathy (0.1.0)\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n  pathy\n";

const ENTRY: &str = "require_relative \"pathy/plain_kind\"\n\nmodule Pathy\n  def self.load_kind(kind)\n    require File.join(File.dirname(__FILE__), \"pathy\", \"#{kind}_kind\")\n  end\nend\n";

const KIND: &str = "module Pathy\n  module PlainKind\n    def self.hi = :plain\n  end\nend\n";

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("Gemfile", "source \"https://rubygems.org\"\ngem \"pathy\"\n")
        .write("Gemfile.lock", LOCK)
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write("roundhouse.yml", "extensions:\n  - config/roundhouse\n")
        .write("config/roundhouse/gems.yml", "source_gems:\n  - {gem: \"pathy\"}\n")
        .write("config/application.rb", "Bundler.require(*Rails.groups)\n")
        .write(&format!("{GEM}/pathy.gemspec"), "Gem::Specification.new do |s|\n  s.name = \"pathy\"\n  s.version = \"0.1.0\"\n  s.summary = \"x\"\n  s.authors = [\"roundhouse\"]\n  s.files = Dir[\"lib/**/*.rb\"]\nend\n")
        .write(&format!("{GEM}/lib/pathy.rb"), ENTRY)
        .write(&format!("{GEM}/lib/pathy/plain_kind.rb"), KIND)
        .write("app/models/kind_report.rb", "module KindReport\n  def self.lines = [Pathy.load_kind(\"plain\"), Pathy::PlainKind.hi]\nend\n")
}

#[test]
fn a_path_a_source_gem_builds_from_its_file_finds_the_file_it_shipped() {
    let (dir, errors) = app().emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let stub = dir.join(".roundhouse/source_gems/pathy-0.1.0/lib/pathy/plain_kind.rb");
    let text = std::fs::read_to_string(&stub).unwrap_or_else(|e| panic!("{}: {e}", stub.display()));
    assert!(text.contains("require_relative"), "{text}");
    let out = emit_and_run::ruby()
        .current_dir(&*dir)
        .args(["-e", "require_relative 'boot'\np KindReport.lines\n"])
        .output()
        .expect("ruby");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert_eq!(stdout, "[false, :plain]\n", "{stderr}");
}

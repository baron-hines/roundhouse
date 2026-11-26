//! What a gem file runs as it loads beyond its declarations, as devise's
//! `Warden::Strategies.add(:database_authenticatable, …)` registers its
//! strategy: Ruby runs it where the file is required, and a file an
//! `autoload` names when the constant is first read. Spinel loads no file,
//! so the boot runs them (`ingest::gem_load_program`); a file nothing
//! loads runs nothing. Compiled as the app's source gem the ruby family
//! runs them in the file's program, not again at boot; installed, the
//! Spinel tree keeps what only they call (`source_gem_shake`).

use super::emit_and_run;
use roundhouse::project::BuildTarget;

fn source_gem() -> emit_and_run::Overlay {
    installed()
        .write("roundhouse.yml", "extensions:\n  - config/roundhouse\n")
        .write("config/roundhouse/gems.yml", "source_gems:\n  - {gem: \"registry\"}\n")
}

fn installed() -> emit_and_run::Overlay {
    let gem = "vendor/bundle/ruby/3.4.0/gems/registry-1.0";
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    debug (1.11.1)\n    registry (1.0)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  registry\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write(
            &format!("{gem}/lib/registry.rb"),
            "require \"registry/table\"\nrequire \"registry/plain\"\n\nmodule Registry\n  autoload :Extra, \"registry/extra\"\n  autoload :Unused, \"registry/unused\"\nend\n",
        )
        .write(
            &format!("{gem}/lib/registry/table.rb"),
            "module Registry\n  class Table\n    @@names = []\n\n    def self.add(name)\n      @@names << name\n    end\n\n    def self.names\n      @@names.join(\",\")\n    end\n  end\nend\n",
        )
        .write(
            &format!("{gem}/lib/registry/plain.rb"),
            "module Registry\n  class Plain\n    def self.label\n      \"plain\"\n    end\n  end\nend\n\nRegistry::Table.add(Registry::Plain.label)\n",
        )
        .write(
            &format!("{gem}/lib/registry/extra.rb"),
            "module Registry\n  class Extra\n    def self.label\n      \"extra\"\n    end\n  end\nend\n\nRegistry::Table.add(Registry::Extra.label)\n",
        )
        .write(
            &format!("{gem}/lib/registry/unused.rb"),
            "module Registry\n  class Unused\n  end\nend\n\nRegistry::Table.add(\"unused\")\n",
        )
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  def self.registered\n    Registry::Extra.label\n    Registry::Table.names\n  end\n\n",
        )
}

const SCRIPT: &str = r#"
got = Article.registered
raise "got #{got.inspect}" unless got == "plain,extra"
puts "gem file statements passed"
"#;

#[test]
fn a_gem_files_statements_join_the_spinel_boot_only() {
    let (tree, errors) = installed().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let boot = std::fs::read_to_string(tree.join("config/initializers.rb")).expect("boot program");
    assert!(boot.contains("Registry::Table.add(Registry::Plain.label)"), "{boot}");
    assert!(boot.contains("Registry::Table.add(Registry::Extra.label)"), "{boot}");
    assert!(!boot.contains("\"unused\""), "{boot}");
    let table = std::fs::read_to_string(tree.join("app/models/registry/table.rb")).expect("Registry::Table");
    assert!(table.contains("def self.add(name)"), "{table}");
    for overlay in [installed(), source_gem()] {
        let (tree, _) = overlay.emit(BuildTarget::Ruby);
        let boot = std::fs::read_to_string(tree.join("config/initializers.rb")).unwrap_or_default();
        assert!(!boot.contains("Registry::Table.add"), "{boot}");
    }
}

#[test]
fn a_gem_files_statements_run_as_it_loads_on_ruby() {
    let run = source_gem().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("gem file statements passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_gem_files_statements_run_at_boot_on_spinel() {
    let run = installed().run_spinel(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("gem file statements passed"), "{}", run.stdout);
}

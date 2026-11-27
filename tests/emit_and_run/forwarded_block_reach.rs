//! A gem's `update(label, &block)` that hands its block on to
//! `add(label, kind, &block)` passes `add` a block only if a block reaches
//! `update`. An untyped `record.update(title: "x")` in the app reaches the
//! gem's `update` (its receiver may be any class), but passes no block, so
//! `kind.class_eval(&block) if block_given?` in `add` is not reached code.

use super::emit_and_run;
use roundhouse::project::BuildTarget;

const REGISTRY: &str = r##"class Registry
  class Base
    def run
      "base"
    end
  end

  class Plain < Base
    def run
      "plain"
    end
  end

  class << self
    def add(label, kind, &block)
      kind.class_eval(&block) if block_given?
      _kinds[label] = kind
    end

    def update(label, &block)
      kind = _kinds[label]
      raise "Unknown kind #{label.inspect}" unless kind
      add(label, kind, &block)
    end

    def _kinds
      @kinds ||= {}
    end
  end

  def self.run(label)
    _kinds[label].new.run
  end
end
"##;

fn app(article: &str) -> emit_and_run::Overlay {
    let gem = "vendor/bundle/ruby/3.4.0/gems/registry-1.0";
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    debug (1.11.1)\n    registry (1.0)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  registry\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write(&format!("{gem}/lib/registry.rb"), &format!("{REGISTRY}\nrequire \"registry/setup\"\n"))
        .write(&format!("{gem}/lib/registry/setup.rb"), "Registry.add(:plain, Registry::Plain)\n")
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", &format!("class Article < ApplicationRecord\n{article}\n"))
}

fn emitted_registry(emitted: &std::path::Path) -> String {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|x| x == "rb") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(emitted, &mut files);
    files.iter().map(|f| std::fs::read_to_string(f).unwrap()).find(|t| t.contains("def self.update(label")).expect("the emitted registry")
}

const BUMP: &str = "  def self.bump(record)\n    record.update(title: \"x\")\n  end\n\n  def self.kind\n    Registry.run(:plain)\n  end\n";

#[test]
fn a_block_forwarded_from_a_method_given_none_is_not_passed() {
    let (emitted, errors) = app(BUMP).emit(BuildTarget::Spinel);
    assert!(errors.iter().all(|e| !e.contains("class_eval")), "{errors:#?}");
    let registry = emitted_registry(&emitted);
    assert!(!registry.contains("block_given?"), "{registry}");
}

#[test]
fn a_block_forwarded_from_a_method_given_one_is_passed() {
    let given = BUMP.replace("record.update(title: \"x\")", "Registry.update(:plain) { def extra = 1 }");
    let (emitted, _) = app(&given).emit(BuildTarget::Spinel);
    let registry = emitted_registry(&emitted);
    assert!(registry.contains("block_given?"), "{registry}");
}

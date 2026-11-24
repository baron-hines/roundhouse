//! A source gem's module body registers its files through its own
//! `def self.register_autoload(name, path)`, whose body calls
//! `autoload(name, path)` (the mail gem's shape). The Spinel tree compiles
//! the registered file as it compiles a literal `autoload`'s, and the helper
//! keeps the rest of its body without the `autoload` Spinel cannot read.

use roundhouse::build_target::BuildTarget;

use super::gem_autoload_helper_contract as gem;

fn emitted_files(tree: &std::path::Path) -> Vec<String> {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rb") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(tree, &mut files);
    files.iter().filter_map(|p| std::fs::read_to_string(p).ok()).collect()
}

fn postbox_module(files: &[String]) -> &str {
    files
        .iter()
        .find(|text| text.contains("module Postbox") && text.contains("def register_autoload") || text.contains("module Postbox") && text.contains("def self.register_autoload"))
        .expect("Postbox's module is not in the Spinel tree")
}

#[test]
fn the_spinel_tree_compiles_the_file_a_helper_registers() {
    let (emitted, errors) = gem::overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let files = emitted_files(&emitted);
    assert!(files.iter().any(|text| text.contains("  class Envelope")), "the registered file is not in the Spinel tree");
    let module = postbox_module(&files);
    assert!(!module.lines().any(|l| l.trim() == "autoload(name, path)"), "the helper still calls autoload with its parameters:\n{module}");
    assert!(module.contains("@autoloads[name] = path"), "the helper lost the rest of its body:\n{module}");
    assert!(module.contains("register_autoload :Envelope, \"postbox/envelope\""), "the body call is gone:\n{module}");
}

/// A call with a path built at run time is not followed: the helper keeps its `autoload`.
#[test]
fn a_helper_also_called_with_a_built_path_keeps_its_autoload() {
    let (emitted, errors) = gem::overlay()
        .edit(
            "vendor/bundle/ruby/3.4.0/gems/postbox-1.0/lib/postbox.rb",
            "  register_autoload :Envelope, \"postbox/envelope\"\n",
            "  require \"postbox/envelope\"\n  register_autoload :Envelope, \"postbox/envelope\"\n  register_autoload :Stamp, \"postbox/\" + \"stamp\"\n",
        )
        .emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let files = emitted_files(&emitted);
    let module = postbox_module(&files);
    assert!(module.lines().any(|l| l.trim() == "autoload(name, path)"), "the helper's autoload was dropped:\n{module}");
}

const POSTBOX: &str = "vendor/bundle/ruby/3.4.0/gems/postbox-1.0/lib/postbox.rb";

/// The helper keeps its `autoload` after `edits` to the gem's module.
fn keeps_its_autoload(edits: &[(&str, &str)]) {
    let mut overlay = gem::overlay().edit(
        POSTBOX,
        "  register_autoload :Envelope, \"postbox/envelope\"\n",
        "  require \"postbox/envelope\"\n  register_autoload :Envelope, \"postbox/envelope\"\n",
    );
    for (from, to) in edits {
        overlay = overlay.edit(POSTBOX, from, to);
    }
    let (emitted, errors) = overlay.emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let files = emitted_files(&emitted);
    let module = postbox_module(&files);
    // A rebound parameter is renamed on emit (`autoload(name, __rh_path)`).
    assert!(module.lines().any(|l| l.trim().starts_with("autoload(name, ")), "the helper's autoload was dropped:\n{module}");
}

/// A built path in an assignment's call is a call of the helper too.
#[test]
fn a_helper_also_called_inside_an_assignment_keeps_its_autoload() {
    keeps_its_autoload(&[("  register_autoload :Envelope, \"postbox/envelope\"\n", "  register_autoload :Envelope, \"postbox/envelope\"\n  @stamp = register_autoload(:Stamp, \"postbox/\" + \"stamp\")\n")]);
}

/// A built path in a `self.` call is a call of the helper too.
#[test]
fn a_helper_also_called_through_self_keeps_its_autoload() {
    keeps_its_autoload(&[("  register_autoload :Envelope, \"postbox/envelope\"\n", "  register_autoload :Envelope, \"postbox/envelope\"\n  self.register_autoload(:Stamp, \"postbox/\" + \"stamp\")\n")]);
}

/// A helper defined in `class << self` is not one source-gem ingest follows.
#[test]
fn a_helper_in_a_singleton_class_body_keeps_its_autoload() {
    keeps_its_autoload(&[(
        "  def self.register_autoload(name, path)\n    @autoloads[name] = path\n    autoload(name, path)\n  end\n",
        "  class << self\n    def register_autoload(name, path)\n      @autoloads[name] = path\n      autoload(name, path)\n    end\n  end\n",
    )]);
}

/// A helper that rebinds its path registers a path its callers do not pass.
#[test]
fn a_helper_that_rebinds_its_path_keeps_its_autoload() {
    keeps_its_autoload(&[(
        "    @autoloads[name] = path\n",
        "    path = \"postbox/\" + path.to_s\n    @autoloads[name] = path\n",
    )]);
}

/// A second definition of the helper is the one Ruby runs, whatever the first one does. The table is a constant
/// here: Spinel refuses a redefinition beside a class-instance-variable initializer.
#[test]
fn a_helper_defined_twice_keeps_its_autoload() {
    keeps_its_autoload(&[
        ("  @autoloads = {}\n", "  AUTOLOADS = {}\n"),
        ("    @autoloads[name] = path\n", "    AUTOLOADS[name] = path\n"),
        ("    @autoloads.keys\n", "    AUTOLOADS.keys\n"),
        (
            "  def self.registered\n",
            "  def self.register_autoload(name, path)\n    path = \"postbox/\" + path.to_s\n    AUTOLOADS[name] = path\n    autoload(name, path)\n  end\n\n  def self.registered\n",
        ),
    ]);
}

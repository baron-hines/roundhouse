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
        .find(|text| text.contains("module Postbox") && text.contains("def self.register_autoload"))
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

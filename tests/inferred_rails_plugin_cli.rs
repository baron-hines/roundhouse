//! A gem the app depends on that depends on `railties` reads as
//! `rails_dependent` from the lockfile alone (#2275). The app's references
//! to its constants are analysis errors, which `--allow-unsupported`
//! downgrades: the tree is written and a reference raises where it runs.
//! Only a `rails_dependent` row a project extension writes refuses the
//! whole target (#2280), and on Roda, which runs no analysis, an inferred
//! row too.

#[path = "support/scratch.rs"]
mod scratch;

use std::path::{Path, PathBuf};
use std::process::Command;

const WIDGET: &str = "class Widget\n  def self.token_for(id)\n    GadgetEngine::Token.find_by(id: id)\n  end\nend\n";

/// real-blog with `gadget-engine` (which depends on railties) locked and
/// depended on, and a model that reads one of its constants.
struct App(PathBuf);

impl App {
    fn new(label: &str) -> Self {
        let root = scratch::dir(&format!("roundhouse-inferred-rails-plugin-{label}"));
        let app = root.join("app");
        copy_tree(roundhouse::fixtures::real_blog(), &app);
        let lock = std::fs::read_to_string(app.join("Gemfile.lock")).unwrap();
        assert_eq!(lock.matches("  specs:\n").count(), 1, "{lock}");
        let lock = lock
            .replacen("  specs:\n", "  specs:\n    gadget-engine (1.0.0)\n      railties (>= 7.0)\n", 1)
            .replacen("DEPENDENCIES\n", "DEPENDENCIES\n  gadget-engine\n", 1);
        std::fs::write(app.join("Gemfile.lock"), lock).unwrap();
        std::fs::write(app.join("app/models/widget.rb"), WIDGET).unwrap();
        App(root)
    }

    fn app(&self) -> PathBuf {
        self.0.join("app")
    }

    fn write(&self, path: &str, content: &str) {
        let path = self.app().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    /// Run the command line: whether it succeeded, its stderr, and the output dir.
    fn emit(&self, target: &str, allow_unsupported: bool) -> (bool, String, PathBuf) {
        let out = self.0.join(format!("{target}-{}", if allow_unsupported { "permissive" } else { "strict" }));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_roundhouse"));
        cmd.args(["--target", target]).arg("-o").arg(&out).arg(self.app())
            .env_remove("ROUNDHOUSE_INGEST_SURVEY")
            .env("ROUNDHOUSE_OBSERVE_SHAPES", "0");
        if allow_unsupported {
            cmd.arg("--allow-unsupported");
        }
        let result = cmd.output().expect("run roundhouse");
        (result.status.success(), String::from_utf8_lossy(&result.stderr).into_owned(), out)
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Whether the run wrote any of a tree.
fn written(out: &Path) -> bool {
    std::fs::read_dir(out).is_ok_and(|mut entries| entries.next().is_some())
}

const INFERRED: &str = "gem gadget-engine it depends on railties";

#[test]
fn an_inferred_rails_plugin_reference_is_an_error_by_default() {
    let app = App::new("strict");
    for target in ["ruby", "spinel", "roda"] {
        let (ok, stderr, out) = app.emit(target, false);
        assert!(!ok, "{target}: {stderr}");
        assert!(stderr.contains("gem_requires_rails") && stderr.contains(INFERRED), "{target}: {stderr}");
        assert!(!written(&out), "{target}: an error run writes no tree");
    }
}

#[test]
fn allow_unsupported_writes_the_tree_past_an_inferred_rails_plugin() {
    let app = App::new("permissive");
    for target in ["ruby", "spinel"] {
        let (ok, stderr, out) = app.emit(target, true);
        assert!(ok, "{target}: {stderr}");
        assert!(stderr.contains("warning") && stderr.contains("gem_requires_rails") && stderr.contains(INFERRED), "{target}: {stderr}");
        assert!(!stderr.contains("roundhouse: gem_requires_rails"), "{target}: {stderr}");
        assert!(out.join("app/models/widget.rb").is_file(), "{target}: no tree written\n{stderr}");
    }
}

/// Roda converts the ingest-shape app and runs no type analysis, so no
/// analysis error reports the reference: the target stays refused there,
/// `--allow-unsupported` or not.
#[test]
fn roda_still_refuses_an_inferred_rails_plugin() {
    let app = App::new("roda");
    let (ok, stderr, out) = app.emit("roda", true);
    assert!(!ok, "{stderr}");
    assert!(stderr.contains("roundhouse: gem_requires_rails") && stderr.contains(INFERRED), "{stderr}");
    assert!(!written(&out), "a refused target writes no tree");
}

/// A row the project writes is its own decision: the target stays refused,
/// `--allow-unsupported` or not.
#[test]
fn an_extension_written_rails_dependent_row_still_refuses_the_target() {
    let app = App::new("extension");
    app.write("roundhouse.yml", "extensions: [config/rh]\n");
    app.write("config/rh/gems.yml", "rails_dependent:\n  - {gem: gadget-engine, why: requires the Rails framework}\n");
    for target in ["ruby", "spinel", "roda"] {
        let (ok, stderr, out) = app.emit(target, true);
        assert!(!ok, "{target}: {stderr}");
        assert!(stderr.contains("a project extension marks gem gadget-engine rails_dependent"), "{target}: {stderr}");
        assert!(!written(&out), "{target}: a refused target writes no tree");
    }
}

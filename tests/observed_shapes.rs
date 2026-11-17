//! The Spinel build of an app whose gem defines methods on, and mixes modules into, a model it holds in a
//! variable (aasm 5.5.1's `klass.send(:define_method, …)`, `base.send(:include, …)`): it boots the CRuby
//! tree once with the shape observer, writes what the gem did where the model's own statement is, reads the
//! gem's statements that did it as `nil`, and reuses the observation while nothing it read changes.

#[path = "support/prerequisite.rs"]
mod prerequisite;

use std::path::{Path, PathBuf};
use std::process::Command;

const VERSION: &str = "5.5.1";

const MODEL: &str = "class Article < ApplicationRecord
  include AASM

  aasm column: :status do
    state :draft, initial: true
    state :published

    event :publish do
      transitions from: :draft, to: :published
    end
  end
";

fn installed_aasm(dir: &Path) -> Option<PathBuf> {
    let out = Command::new("ruby")
        .args([
            "-e",
            &format!("print Gem::Specification.find_by_name('aasm', '{VERSION}').gem_dir"),
        ])
        .current_dir(dir)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| PathBuf::from(String::from_utf8_lossy(&out.stdout).into_owned()))
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if src.is_dir() {
            copy(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

fn edit(path: &Path, from: &str, to: &str) {
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains(from), "{} lacks {from:?}", path.display());
    std::fs::write(path, text.replacen(from, to, 1)).unwrap();
}

/// real-blog with aasm in its bundle and the state machine on `Article`.
fn app(root: &Path) -> Option<PathBuf> {
    let app = root.join("app");
    copy(&roundhouse::fixtures::real_blog(), &app);
    let Some(gem) = installed_aasm(&app) else {
        prerequisite::missing(&format!("ruby with aasm {VERSION} installed"));
        return None;
    };
    copy(
        &gem,
        &app.join(format!("vendor/bundle/ruby/3.4.0/gems/aasm-{VERSION}")),
    );
    edit(
        &app.join("Gemfile.lock"),
        "\n    rack (",
        &format!("\n    aasm ({VERSION})\n      concurrent-ruby (~> 1.0)\n    rack ("),
    );
    edit(
        &app.join("Gemfile.lock"),
        "DEPENDENCIES\n",
        "DEPENDENCIES\n  aasm\n",
    );
    let gemfile = std::fs::read_to_string(app.join("Gemfile")).unwrap() + "gem \"aasm\"\n";
    std::fs::write(app.join("Gemfile"), gemfile).unwrap();
    std::fs::create_dir_all(app.join(".bundle")).unwrap();
    std::fs::write(
        app.join(".bundle/config"),
        "---\nBUNDLE_PATH: \"vendor/bundle\"\n",
    )
    .unwrap();
    edit(
        &app.join("db/schema.rb"),
        "  create_table \"articles\", force: :cascade do |t|\n",
        "  create_table \"articles\", force: :cascade do |t|\n    t.string \"status\"\n",
    );
    edit(
        &app.join("app/models/article.rb"),
        "class Article < ApplicationRecord\n",
        MODEL,
    );
    Some(app)
}

/// A scratch directory of its own, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "roundhouse-observed-shapes-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn spinel(app: &Path, out: &Path) -> String {
    let run = Command::new(env!("CARGO_BIN_EXE_roundhouse"))
        .args(["--target", "spinel", "--allow-unsupported"])
        .arg(app)
        .arg("-o")
        .arg(out)
        .output()
        .expect("spawn roundhouse");
    String::from_utf8_lossy(&run.stderr).into_owned()
}

#[test]
fn a_gem_that_shapes_a_held_model_is_observed_and_written_where_the_model_is() {
    let root = Scratch::new("aasm");
    let Some(app) = app(root.path()) else { return };
    let out = root.path().join("spinel");
    let log = spinel(&app, &out);
    assert!(
        !log.contains("observing the boot did not answer it"),
        "{log}"
    );
    let model = std::fs::read_to_string(out.join("app/models/article.rb")).unwrap();
    for written in [
        "include AASM::Persistence::ActiveRecordPersistence",
        "extend ::AASM::ClassMethods",
        "STATE_DRAFT = :draft",
        "def publish!(*args, &block)",
        "aasm_fire_event(:default, :publish, { persist: true }, *args, &block)",
    ] {
        assert!(
            model.contains(written),
            "{written} is not in the Spinel model:\n{model}"
        );
    }
    let base = std::fs::read_to_string(out.join("app/models/aasm/base.rb")).unwrap();
    for done in [
        "klass.send(:define_method, method_name, method_definition)",
        "klass.const_set(const_name, name)",
    ] {
        assert!(
            !base.contains(done),
            "aasm's own `{done}` still runs:\n{base}"
        );
    }

    // A file the gem requires by a path it builds (`require_persistence :active_record`) is one the boot loaded, so it is carried.
    assert!(out.join("app/models/aasm/persistence/active_record_persistence.rb").exists());
    // Its `class_eval do scope … end` goes with the observed `scope`, and the branch this Rails rules out goes too.
    let created = model.split("def self.aasm_create_scope").nth(1).expect("aasm_create_scope in the Spinel model");
    let created = &created[..created.find("\n  def ").unwrap_or(created.len())];
    assert!(!created.contains("class_eval") && !created.contains("named_scope"), "{created}");

    // Nothing it read changed: the second build reuses the observation, without booting the CRuby tree.
    let again = spinel(&app, &root.path().join("spinel-again"));
    assert!(!again.contains("(ruby)"), "{again}");
    assert_eq!(
        std::fs::read_to_string(root.path().join("spinel-again/app/models/article.rb")).unwrap(),
        model
    );
}

#[test]
fn an_app_whose_gems_shape_no_held_class_boots_nothing() {
    let root = Scratch::new("plain");
    let app = root.path().join("app");
    copy(&roundhouse::fixtures::real_blog(), &app);
    let log = spinel(&app, &root.path().join("spinel"));
    assert!(!log.contains("(ruby)"), "{log}");
    assert!(!app.join(roundhouse::observed_shapes::CACHE).exists());
}

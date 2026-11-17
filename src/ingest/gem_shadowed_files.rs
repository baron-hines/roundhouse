//! An app file at the autoload path of a constant a source gem already
//! defines is a file Rails never loads.
//!
//! Zeitwerk sets an autoload for `Shelf::Box` from `app/models/shelf/box.rb`
//! only while `Shelf::Box` is undefined, and eager loading skips a file
//! whose constant is already defined the same way. A gem `Bundler.require`
//! loads defines its constants before the app's autoloaders are set up, so
//! the app's file at that path is never read, with eager loading or
//! without. A file a boot file names (`require Rails.root.join("app/…")`)
//! is still loaded by that `require`, so it stays.
//!
//! The file is hidden from the ingest's tree ([`HiddenVfs`]) rather than
//! skipped by each pass, so no pass reads it.

use std::path::{Path, PathBuf};

use crate::det::HashSet;
use crate::vfs::Vfs;

use super::source_gems::SourceGem;

/// The app files under an autoload root whose constant, as Zeitwerk names
/// it from the file's path, one of `gems`' `lib/` files defines.
pub(super) fn shadowed<V: Vfs + ?Sized>(vfs: &V, dir: &Path, roots: &[PathBuf], gems: &[SourceGem]) -> HashSet<PathBuf> {
    let mut out = HashSet::default();
    if gems.is_empty() {
        return out;
    }
    let mut gem_files = Vec::new();
    for gem in gems {
        let lib = gem.mount.join("lib");
        if !vfs.is_dir(&lib) {
            continue;
        }
        // Not a generator: `rails generate` loads it, the app's boot does not.
        gem_files.extend(
            super::app::read_rb_files(vfs, &lib)
                .unwrap_or_default()
                .into_iter()
                .filter(|f| !f.strip_prefix(&lib).is_ok_and(|rel| rel.starts_with("generators"))),
        );
    }
    let defined = super::railtie_middleware::defined_constants(vfs, &gem_files);
    if defined.is_empty() {
        return out;
    }
    let in_gem = |path: &Path| gems.iter().any(|gem| path.starts_with(&gem.mount));
    let mut autoload_roots: Vec<PathBuf> = Vec::new();
    for root in roots {
        let root = dir.join(root);
        if in_gem(&root) {
            continue;
        }
        for entry in vfs.read_dir(&root).unwrap_or_default() {
            if !vfs.is_dir(&entry) {
                continue;
            }
            // `app/models/concerns` is a root of its own, as Rails adds it.
            let concerns = entry.join("concerns");
            if vfs.is_dir(&concerns) {
                autoload_roots.push(concerns);
            }
            autoload_roots.push(entry);
        }
    }
    if let Ok(source) = vfs.read(&dir.join("config/application.rb")) {
        autoload_roots.extend(super::app::extract_autoload_path_roots(&source).into_iter().map(|root| dir.join(root)));
    }
    autoload_roots.retain(|root| !in_gem(root) && vfs.is_dir(root));
    autoload_roots.sort();
    autoload_roots.dedup();
    let boot_text = boot_text(vfs, dir);
    let mut seen: HashSet<PathBuf> = HashSet::default();
    for root in &autoload_roots {
        for file in super::app::read_rb_files(vfs, root).unwrap_or_default() {
            if !seen.insert(file.clone()) {
                continue;
            }
            // The innermost root names it: `app/models/concerns/taggable.rb` is `Taggable`.
            let Some(rel) = autoload_roots
                .iter()
                .filter_map(|r| file.strip_prefix(r).ok())
                .min_by_key(|rel| rel.components().count())
                .and_then(|rel| rel.to_str())
                .and_then(|rel| rel.strip_suffix(".rb"))
            else {
                continue;
            };
            if !defined.contains(&crate::naming::camelize_path(rel)) {
                continue;
            }
            let from_app = file.strip_prefix(dir).ok().and_then(Path::to_str).map(|p| p.trim_end_matches(".rb").to_string());
            if from_app.is_some_and(|stem| boot_text.iter().any(|text| text.contains(&stem))) {
                continue;
            }
            out.insert(file);
        }
    }
    out
}

/// What the boot reads before the app's constants autoload: a file named
/// here may be required by path.
fn boot_text<V: Vfs + ?Sized>(vfs: &V, dir: &Path) -> Vec<String> {
    let config = dir.join("config");
    let mut files: Vec<PathBuf> = ["application.rb", "environment.rb", "boot.rb"].iter().map(|f| config.join(f)).collect();
    for sub in ["environments", "initializers"] {
        let sub = config.join(sub);
        if vfs.is_dir(&sub) {
            files.extend(super::app::read_rb_files(vfs, &sub).unwrap_or_default());
        }
    }
    files.iter().filter_map(|f| vfs.read_to_string(f).ok()).collect()
}

/// `inner` without the [`shadowed`] files.
pub(super) struct HiddenVfs<'a, V: Vfs + ?Sized> {
    pub(super) inner: &'a V,
    pub(super) hidden: HashSet<PathBuf>,
}

impl<V: Vfs + ?Sized> HiddenVfs<'_, V> {
    fn check(&self, path: &Path) -> std::io::Result<()> {
        if self.hidden.contains(path) {
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "a source gem defines this file's constant, so Rails never loads it"))
        } else {
            Ok(())
        }
    }
}

impl<V: Vfs + ?Sized> Vfs for HiddenVfs<'_, V> {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        self.check(path)?;
        self.inner.read(path)
    }

    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        self.check(path)?;
        self.inner.read_to_string(path)
    }

    fn read_dir(&self, path: &Path) -> std::io::Result<Vec<PathBuf>> {
        let mut entries = self.inner.read_dir(path)?;
        if !self.hidden.is_empty() {
            entries.retain(|entry| !self.hidden.contains(entry));
        }
        Ok(entries)
    }

    fn exists(&self, path: &Path) -> bool {
        !self.hidden.contains(path) && self.inner.exists(path)
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.inner.is_dir(path)
    }

    fn is_symlink(&self, path: &Path) -> bool {
        self.inner.is_symlink(path)
    }

    fn canonical(&self, path: &Path) -> Option<PathBuf> {
        self.inner.canonical(path)
    }
}

//! Test-owned scratch directories (#1734).
//!
//! `scratch::dir(prefix)` creates a fresh directory under the system temp
//! directory with an exclusive `mkdir` (mode 0700 on Unix) and returns it.
//! A path that already exists — a stale tree from an earlier run, another
//! test binary's live tree, or something another local user planted (a
//! directory, or a symlink pointing elsewhere) — is never adopted: the name
//! is skipped and the next counter value tried. Inside the returned root,
//! `create_dir_all` for nested fixture folders is fine; the root itself is
//! ours.
//!
//! The returned path is canonical, so paths a child Ruby reports
//! (`__dir__` is a realpath) match on macOS, where `$TMPDIR` sits under
//! `/var -> /private/var`.
//!
//! Every directory made here is removed when the test binary exits (on
//! Unix), passed or failed, unless `ROUNDHOUSE_KEEP_EMITTED` is set. Left behind, a full
//! suite fills `/tmp`'s inodes. A test that is done with a large tree
//! sooner can still call `remove_dir_all` itself.
//!
//! Intentional exceptions: directories under `CARGO_TARGET_TMPDIR` live in
//! the build's own `target/` tree, which other users cannot write, and keep
//! their deterministic names.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The roots this copy of the module made, removed at exit.
static CREATED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Remove `dir` when the process exits, unless `ROUNDHOUSE_KEEP_EMITTED`
/// is set. libtest runs the tests on other threads and then exits, failed
/// or not, so `atexit` sees every tree; a test's panic does not skip it.
fn remove_at_exit(dir: &std::path::Path) {
    static REGISTER: std::sync::Once = std::sync::Once::new();
    #[cfg(unix)]
    REGISTER.call_once(|| {
        extern "C" fn remove_created() {
            if std::env::var_os("ROUNDHOUSE_KEEP_EMITTED").is_some() {
                return;
            }
            let created = std::mem::take(&mut *CREATED.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
            for dir in created {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
        // SAFETY: registers a plain function with no captured state.
        unsafe { libc::atexit(remove_created) };
    });
    CREATED.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(dir.to_path_buf());
}

/// A fresh, exclusively created directory named `{prefix}-{pid}-{n}`.
pub fn dir(prefix: &str) -> PathBuf {
    let base = std::env::temp_dir()
        .canonicalize()
        .expect("canonicalize the system temp directory");
    dir_in(&base, prefix)
}

/// As [`dir`], under `base` instead of the system temp directory.
pub fn dir_in(base: &std::path::Path, prefix: &str) -> PathBuf {
    // The base is shared, not ours (the system temp directory or
    // `CARGO_TARGET_TMPDIR`), so making it if it is missing adopts nothing:
    // only the `{prefix}-…` root below is created exclusively. A test
    // binary run from an archived build can start before anything has made
    // `CARGO_TARGET_TMPDIR`.
    std::fs::create_dir_all(base)
        .unwrap_or_else(|error| panic!("create scratch base {}: {error}", base.display()));
    // Several test binaries, and several copies of this module inside one
    // binary, share a PID: the counter only spreads names, exclusivity
    // comes from `create`.
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    for _ in 0..10_000 {
        let dir = base.join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&dir) {
            Ok(()) => {
                remove_at_exit(&dir);
                return dir;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("create scratch directory {}: {error}", dir.display()),
        }
    }
    panic!("no free scratch directory name for {prefix:?} under {}", base.display())
}

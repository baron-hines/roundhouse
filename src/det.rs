//! Hash collections whose iteration order is the same on every run.
//!
//! `std`'s `RandomState` seeds each process differently, so anything that
//! walks a `HashMap` (a fixpoint worklist, slot updates, diagnostic
//! output) could produce a different, equally valid answer from one run
//! to the next. These aliases use rustc's `FxHasher`, which has no seed:
//! same API (construct with `default()` or `with_capacity_and_hasher`),
//! and a given insertion sequence always iterates in the same order.
//!
//! It is also faster than std's SipHash. On a large app (21,791 Ruby
//! files) SipHash was a sixth of `check`'s analyze time in a `perf`
//! profile. Fx mixes weakly, which only matters for attacker-chosen keys
//! at scale; the analyzer hashes identifiers and types from the app it is
//! compiling.
//!
//! Use these where a map's order can reach output or a fixpoint. Order
//! that must not depend on insertion at all still wants `BTreeMap` or
//! `IndexMap`.

use rustc_hash::FxHasher as DefaultHasher;
use std::hash::BuildHasherDefault;

/// The seedless hasher behind [`HashMap`] and [`HashSet`].
pub type DetState = BuildHasherDefault<DefaultHasher>;

/// `std::collections::HashMap` with a deterministic hasher.
pub type HashMap<K, V> = std::collections::HashMap<K, V, DetState>;

/// `std::collections::HashSet` with a deterministic hasher.
pub type HashSet<T> = std::collections::HashSet<T, DetState>;

#[cfg(test)]
mod tests {
    use super::*;

    /// Two maps filled in the same order iterate in the same order. With
    /// `RandomState` each map draws its own seed, so this fails almost
    /// always for 200 keys. Because the hasher has no seed at all, the
    /// same order also holds in a fresh process, which a unit test cannot
    /// observe directly.
    #[test]
    fn same_insertion_order_gives_same_iteration_order() {
        let fill = || {
            let mut map: HashMap<String, usize> = HashMap::default();
            for i in 0..200 {
                map.insert(format!("key{i}"), i);
            }
            map
        };
        let (a, b) = (fill(), fill());
        assert!(a.iter().eq(b.iter()));

        let set_of = || -> HashSet<usize> { (0..200).map(|i| i * 7).collect() };
        assert!(set_of().iter().eq(set_of().iter()));
    }

    /// Library `src/` must not reintroduce `use std::collections::HashMap`
    /// / `HashSet`. Tree input uses [`crate::vfs::FileTree`]; emitted Rust
    /// may still *print* the std path inside string literals. Bins are
    /// separate crates and are not scanned.
    #[test]
    fn library_does_not_import_std_hash_collections() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        fn walk(dir: &std::path::Path, offenders: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).expect("read src") {
                let entry = entry.expect("dirent");
                let path = entry.path();
                if path.is_dir() {
                    if path.file_name().and_then(|s| s.to_str()) == Some("bin") {
                        continue;
                    }
                    walk(&path, offenders);
                    continue;
                }
                if path.extension().and_then(|s| s.to_str()) != Some("rs") {
                    continue;
                }
                if path.ends_with("det.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).expect("read rs");
                for (i, line) in text.lines().enumerate() {
                    let trimmed = line.trim_start();
                    if trimmed.starts_with("//")
                        || trimmed.starts_with("///")
                        || trimmed.starts_with('*')
                        || trimmed.contains('"')
                        || trimmed.contains('\'')
                    {
                        continue;
                    }
                    let imports_std_hash = trimmed.starts_with("use std::collections::HashMap")
                        || trimmed.starts_with("use std::collections::HashSet")
                        || (trimmed.starts_with("use std::collections::{")
                            && (trimmed.contains("HashMap") || trimmed.contains("HashSet")));
                    if imports_std_hash {
                        offenders.push(format!("{}:{}", path.display(), i + 1));
                    }
                }
            }
        }
        walk(&root, &mut offenders);
        assert!(
            offenders.is_empty(),
            "std HashMap/HashSet imports in library src (use crate::det or FileTree):\n{}",
            offenders.join("\n")
        );
    }
}

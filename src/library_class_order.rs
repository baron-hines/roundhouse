//! Order [`LibraryClass`]es so same-set parents and includes precede dependents.
//!
//! File-level ingest and nested Ruby emit both need this: a subclass that
//! names another class in the same file must load after that class, or Ruby
//! raises `NameError` while defining it. Cycles cannot be Ruby either; they
//! keep source order.

use crate::det::HashSet;

/// Topologically order `items` by dependency names that also appear in the set.
///
/// `name_of` is the item's identity in the set. `deps_of` returns the resolved
/// dependency names (parents and includes) that should wait on siblings —
/// callers map bare / rooted spellings first. When a cycle remains, the rest
/// keep their relative order.
pub fn order_by_deps<T>(
    mut pending: Vec<T>,
    name_of: impl Fn(&T) -> &str,
    deps_of: impl Fn(&T) -> Vec<String>,
) -> Vec<T> {
    let names: HashSet<String> = pending.iter().map(|item| name_of(item).to_string()).collect();
    let mut ordered = Vec::with_capacity(pending.len());
    let mut placed = HashSet::default();
    while !pending.is_empty() {
        let before = pending.len();
        let mut deferred = Vec::new();
        for item in pending.drain(..) {
            let waits_for = deps_of(&item)
                .into_iter()
                .any(|dep| names.contains(&dep) && !placed.contains(&dep));
            if waits_for {
                deferred.push(item);
                continue;
            }
            placed.insert(name_of(&item).to_string());
            ordered.push(item);
        }
        if deferred.len() == before {
            ordered.extend(deferred);
            break;
        }
        pending = deferred;
    }
    ordered
}

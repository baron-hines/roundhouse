use std::path::PathBuf;
use std::sync::RwLock;

use crate::det::HashMap;

// Not a thread-local: `par_render`'s workers ask it too.
static PLACED: RwLock<Option<HashMap<String, String>>> = RwLock::new(None);

pub(crate) fn class_anchor(name: &str) -> String {
    if let Ok(placed) = PLACED.read()
        && let Some(anchor) = placed.as_ref().and_then(|m| m.get(name))
    {
        return anchor.clone();
    }
    format!("app/models/{}", crate::naming::underscore(name))
}

pub(crate) fn class_rb(name: &str) -> PathBuf {
    PathBuf::from(format!("{}.rb", class_anchor(name)))
}

pub(crate) fn class_rbs(name: &str) -> PathBuf {
    PathBuf::from(format!("{}.rbs", class_anchor(name)))
}

pub(crate) fn is_class_file(path: &str) -> bool {
    if path.starts_with("app/models/") {
        return true;
    }
    let anchor = path.strip_suffix(".rb").or_else(|| path.strip_suffix(".rbs")).unwrap_or(path);
    PLACED.read().is_ok_and(|placed| placed.as_ref().is_some_and(|m| m.values().any(|a| a == anchor)))
}

#[allow(dead_code)]
pub(crate) fn with_placed<R>(placed: HashMap<String, String>, f: impl FnOnce() -> R) -> R {
    let prev = PLACED.write().map(|mut p| p.replace(placed)).unwrap_or(None);
    let r = f();
    if let Ok(mut p) = PLACED.write() {
        *p = prev;
    }
    r
}

//! Owned-item work within an existing phase; the caller replays effects in order.
use std::sync::Mutex;

pub(super) fn map<T: Send, R: Send>(items: &mut [T], f: impl Fn(&mut T) -> R + Sync) -> Vec<R> {
    if crate::par::workers_for(items.len()) == 1 {
        return items.iter_mut().map(f).collect();
    }
    // Each job hands its exclusive item borrow to exactly one worker.
    let jobs: Vec<_> = items.iter_mut().map(Mutex::new).collect();
    let inflections = crate::naming::app_inflections();
    let plans = crate::lower::i18n_translate::plan_cache_snapshot();
    crate::par::map_init(
        &jobs,
        || {
            crate::naming::install_app_inflections(inflections.clone());
            crate::lower::i18n_translate::install_plan_cache(plans.clone())
        },
        |_, job| f(&mut **job.lock().unwrap()),
    )
}

/// [`map`] over shared items: a pure read of each, results in input order.
pub(super) fn map_ref<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    if crate::par::workers_for(items.len()) == 1 {
        return items.iter().map(f).collect();
    }
    let inflections = crate::naming::app_inflections();
    let plans = crate::lower::i18n_translate::plan_cache_snapshot();
    crate::par::map_init(
        items,
        || {
            crate::naming::install_app_inflections(inflections.clone());
            crate::lower::i18n_translate::install_plan_cache(plans.clone())
        },
        |_, item| f(item),
    )
}

/// `a` here and `b` on a worker, as [`crate::par::join`], with the app's
/// inflections and the i18n plan cache installed on the worker as [`map`]
/// installs them.
pub(super) fn join<A: Send, B: Send>(a: impl FnOnce() -> A + Send, b: impl FnOnce() -> B + Send) -> (A, B) {
    let inflections = crate::naming::app_inflections();
    let plans = crate::lower::i18n_translate::plan_cache_snapshot();
    crate::par::join(a, move || {
        crate::naming::install_app_inflections(inflections);
        let _plans = crate::lower::i18n_translate::install_plan_cache(plans);
        b()
    })
}

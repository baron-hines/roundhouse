//! Rendering independent items of the Ruby emit on worker threads, in
//! the output a serial run writes.
//!
//! A render reads ambient state that lives in thread-locals, and a fresh
//! worker sees defaults of all of it. [`map`] takes each as the calling
//! thread holds it and gives every worker the same: the emit settings
//! (`expr::Settings`), the source markers, the app's inflections and
//! locales, the i18n plan cache, the SQL dialect, and lookup indexes
//! over the same classes. Diagnostics are collected per item and pushed
//! to the caller's buffer in item order.
//!
//! What a render must not do, and what keeps the output a serial run's
//! whichever worker takes an item: draw a read id or hoist a temporary
//! (their names follow the order bodies are lowered in; [`map`] asserts
//! no item did), or write state a later item reads. A pass that needs
//! either is lowering, and stays serial.

use crate::dialect::LibraryClass;
use crate::App;

use super::{expr, library, source_markers};

/// `render` over `items`, one result per item in item order, on as many
/// threads as `par::workers_for` gives. One worker, or a caller recording
/// the classes it renders, is a plain loop on the calling thread.
pub(super) fn map<T: Sync, R: Send>(
    app: &App,
    classes: &[LibraryClass],
    items: &[T],
    render: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    // A test counting index builds or reading the uncached scans counts
    // on its own thread.
    #[cfg(test)]
    if library::lookups_uncached() {
        return items.iter().map(render).collect();
    }
    if crate::par::workers_for(items.len()) == 1 || library::recording_rendered() {
        return items.iter().map(render).collect();
    }
    let settings = expr::Settings::capture();
    let sources = source_markers::snapshot();
    let inflections = crate::naming::app_inflections();
    let locales = crate::runtime_loader::app_locales_snapshot();
    let plans = crate::lower::i18n_translate::plan_cache_snapshot();
    let dialect = crate::emit::shared::schema_sql::Dialect::current();
    let done = crate::par::map_init(
        items,
        || {
            settings.install();
            source_markers::install(sources.clone());
            crate::naming::install_app_inflections(inflections.clone());
            crate::runtime_loader::install_app_locales(locales.clone());
            (crate::lower::i18n_translate::install_plan_cache(plans.clone()), library::install_worker_lookup_indexes(app, classes))
        },
        |_, item| {
            let rendered = crate::emit::diagnostics::scope(|| dialect.scope(|| render(item)));
            assert_eq!(crate::lower::arel::visitor::read_ids_drawn_here(), 0, "a parallel render drew a read id");
            assert_eq!(crate::lower::arel::hoisted_temporaries(), 0, "a parallel render hoisted a temporary");
            rendered
        },
    );
    done.into_iter()
        .map(|(result, diagnostics)| {
            diagnostics.into_iter().for_each(crate::emit::diagnostics::push);
            result
        })
        .collect()
}

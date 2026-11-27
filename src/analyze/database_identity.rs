//! Database-identity refusals (`crate::database`): every model-to-database
//! routing the emitted runtime does not carry (`ingest::database_ownership`),
//! an error at its source span, so an app that needs a database the
//! runtime does not open is reported unsupported instead of silently
//! running on the primary; and, as warnings, each configuration in
//! `config/database.yml` that build-time resolution cannot read
//! (`ingest::database_yml`).

use crate::diagnostic::Diagnostic;
use crate::App;

pub(super) fn diagnose(app: &App) -> Vec<Diagnostic> {
    app.database_refusals
        .iter()
        .map(|refusal| {
            let mut diagnostic = Diagnostic::unsupported(refusal.span, None, refusal.construct.as_str(), refusal.detail.clone());
            if refusal.warning {
                diagnostic.severity = crate::diagnostic::Severity::Warning;
            }
            diagnostic
        })
        .collect()
}

use roundhouse::analyze::Analyzer;
use roundhouse::diagnostic::Severity;
use roundhouse::ident::{ClassId, Symbol};
use roundhouse::project::BuildTarget;
use roundhouse::ty::Ty;

#[path = "support/errno_ewouldblock.rs"]
mod contract;
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

fn app(source: &str) -> roundhouse::App {
    let source = source.replace(
        "class WouldBlockProbe\n",
        "class WouldBlockProbe < ApplicationRecord\n",
    );
    let tree = [
        (
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n",
        ),
        ("db/schema.rb", "ActiveRecord::Schema.define do\nend\n"),
        ("app/models/would_block_probe.rb", source.as_str()),
    ]
    .into_iter()
    .map(|(path, source)| (std::path::PathBuf::from(path), source.as_bytes().to_vec()))
    .collect();
    roundhouse::ingest::ingest_app_from_tree(tree).expect("ingest")
}

#[test]
fn ewouldblock_has_the_existing_exception_methods() {
    let mut app = app(contract::SOURCE);
    let analyzer = Analyzer::new(&app);
    let info = &analyzer.class_registry()[&ClassId(Symbol::from("Errno::EWOULDBLOCK"))];
    assert_eq!(
        info.instance_methods.get(&Symbol::from("message")),
        Some(&Ty::Str)
    );
    let mut diagnostics = roundhouse::session::analyze_and_lower(&mut app);
    diagnostics.extend(roundhouse::analyze::diagnose(&app));
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn ewouldblock_executes_after_ruby_emission() {
    contract::overlay()
        .run_ruby(contract::ASSERTIONS)
        .assert_passes();
}

#[test]
fn spinel_emission_preserves_the_platform_errno_class() {
    let (emitted, errors) = contract::overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    let source = std::fs::read_to_string(emitted.join("app/models/would_block_probe.rb")).unwrap();
    assert!(source.contains("Errno::EWOULDBLOCK"), "{source}");
    assert!(
        !source.contains("roundhouse: constant not supported"),
        "{source}"
    );
}

#[test]
fn unknown_errno_constants_and_exception_methods_remain_reported() {
    for (source, expected) in [
        (
            contract::SOURCE.replace("EWOULDBLOCK", "EWOULDBLOCK_NOT_REAL"),
            "EWOULDBLOCK_NOT_REAL",
        ),
        (
            contract::SOURCE.replace("error.message", "error.no_such_errno_method"),
            "no_such_errno_method",
        ),
    ] {
        let mut app = app(&source);
        Analyzer::new(&app).analyze(&mut app);
        let diagnostics = roundhouse::analyze::diagnose(&app);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.severity == Severity::Error && d.message.contains(expected)),
            "{expected}: {diagnostics:?}"
        );
    }
}

#[test]
#[ignore = "requires Spinel; run in its CI lane"]
fn ewouldblock_executes_natively_with_rbs() {
    contract::overlay()
        .run_spinel_with_rbs(contract::ASSERTIONS)
        .assert_passes();
}

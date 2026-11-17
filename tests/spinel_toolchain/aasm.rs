use super::aasm_gem::{app, KNOWN_GAPS, SCRIPT};

/// Beyond the Ruby lane's: the gem's `__send__` of a built name (aasm.rb's `binding_event`, which this app sets on no event)
/// and the `ruby2_keywords` the boot's observation writes beside each event method. This flow
/// reaches neither.
const SPINEL_GAPS: [&str; 2] = [
    "dynamic send not supported (spinel): `__send__` calls a method whose name is built when it runs",
    "ruby2_keywords not supported (all targets)",
];

/// The aasm app of the Ruby lane's test (emit_and_run::aasm_source), built through the binary, which observes
/// what aasm does to the model at boot, and run natively.
#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn aasm_fires_events_and_persists_the_state_natively() {
    let Some(app) = app() else { return };
    let script = format!(
        "Db.configure(\":memory:\")\nSchema.statements.each {{ |sql| Db.exec(sql) }}\nActiveRecord.adapter = SqliteAdapter\n{SCRIPT}"
    );
    let run = app.run_spinel_observed(&script);
    let known = |e: &&String| KNOWN_GAPS.iter().chain(SPINEL_GAPS.iter()).any(|gap| e.contains(gap));
    let unknown: Vec<_> = run.errors.iter().filter(|e| !known(e)).collect();
    for gap in SPINEL_GAPS {
        assert!(run.errors.iter().any(|e| e.contains(gap)), "{gap} is answered now: drop it from SPINEL_GAPS\n{:#?}", run.errors);
    }
    assert!(run.program_succeeded() && unknown.is_empty(), "{unknown:#?}\n{}\n{}\n{}\n{}", run.status, run.emitted.display(), run.stdout, run.stderr);
    assert!(run.stdout.contains("aasm passed"), "{}", run.stdout);
}

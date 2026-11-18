//! aasm compiled from its installed source (the app is support/aasm_gem.rs's): `include AASM` gives the model `aasm` (the gem's `included` hook
//! extends it with `AASM::ClassMethods`), and its `aasm column: … do … end` block runs as the class loads. The
//! record starts in the initial state (the hook's `after_initialize`), fires an event, refuses one its state
//! does not allow, persists the state column and answers the state's scope, as aasm 5.5.1 answers on Rails.

use super::aasm_gem::{app, KNOWN_GAPS, SCRIPT};

#[test]
fn aasm_fires_events_and_persists_the_state_on_ruby() {
    let Some(app) = app() else { return };
    let run = app.run_ruby(SCRIPT);
    let unknown: Vec<_> = run.errors.iter().filter(|e| !KNOWN_GAPS.iter().any(|gap| e.contains(gap))).collect();
    assert!(unknown.is_empty(), "{unknown:#?}");
    for gap in KNOWN_GAPS {
        assert!(run.errors.iter().any(|e| e.contains(gap)), "{gap} is answered now: drop it from KNOWN_GAPS\n{:#?}", run.errors);
    }
    assert!(run.program_succeeded(), "{}\n{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("aasm passed"), "{}", run.stdout);
}

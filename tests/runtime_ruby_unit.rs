//! Framework Ruby unit-test gate.
//!
//! Runs `runtime/ruby/test/**/*_test.rb` under stock CRuby and asserts
//! every test passes. This is the source-level gate on the framework
//! Ruby that all targets transpile from — bugs caught here surface
//! before they reach any target's runtime, with a clean Ruby stack
//! pointing at the framework source instead of cascading through
//! transpile.
//!
//! Not `#[ignore]`d: CRuby + Minitest are essentially zero-overhead
//! (45 tests in ~13ms locally), so this gates every `cargo test`
//! invocation. Cheap enough to run unconditionally.
//!
//! Skipped when CRuby isn't available — covers contributors who
//! haven't set up Ruby; CI's setup-ruby step ensures the gate fires
//! there.
//!
//! Run:
//!     cargo test --test runtime_ruby_unit -- --nocapture
//! Or directly:
//!     cd runtime/ruby && rake test

use std::path::Path;
use std::process::Command;

#[test]
fn framework_ruby_tests_pass() {
    let runtime_ruby = Path::new("runtime/ruby");
    assert!(
        runtime_ruby.is_dir(),
        "expected runtime/ruby/ to exist; cwd is {}",
        std::env::current_dir().unwrap().display(),
    );

    // Probe for CRuby. Skip with a clear message rather than fail
    // when it's missing — local contributors without Ruby installed
    // shouldn't be blocked by this gate.
    if Command::new("ruby").arg("--version").output().is_err() {
        eprintln!(
            "skipping: CRuby not available on PATH \
             (install Ruby >= 3.2 to run framework unit tests)"
        );
        return;
    }

    // Minitest's `autorun` runs every loaded test file at exit. Keep
    // the suite in one process, except the Active Support differential:
    // it needs to load Rails' I18n before test_helper loads the runtime's
    // I18n shim, so it must run in a fresh process.
    let output = Command::new("ruby")
        .arg("-Itest")
        .arg("-e")
        .arg(
            "Dir[File.join('test', '**', '*_test.rb')].sort.reject { |f| f.end_with?('/active_support_inflections_test.rb') }.each { |f| require File.expand_path(f) }"
        )
        .current_dir(runtime_ruby)
        .output()
        .expect("invoke ruby");

    assert!(
        output.status.success(),
        "framework Ruby tests failed:\n\
         === stdout ===\n{}\n\
         === stderr ===\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let inflections = Command::new("ruby")
        .arg("-Itest")
        .arg("test/active_support_inflections_test.rb")
        .current_dir(runtime_ruby)
        .output()
        .expect("invoke isolated Active Support inflections test");

    assert!(
        inflections.status.success(),
        "isolated Active Support inflections test failed:\n\
         === stdout ===\n{}\n\
         === stderr ===\n{}",
        String::from_utf8_lossy(&inflections.stdout),
        String::from_utf8_lossy(&inflections.stderr),
    );
}

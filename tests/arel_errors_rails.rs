//! `Arel::BindError` has Rails' parent (arel/errors.rb: `class BindError <
//! ArelError`, `class ArelError < StandardError`), so an app's
//! `rescue Arel::ArelError` catches a bind error from `Arel.sql`. The
//! runtime's `Arel` shim answers the same ancestry as Rails' `arel` gem.
//! Skipped when CRuby or the activerecord gem is missing.

#[path = "support/prerequisite.rs"]
mod prerequisite;
use std::path::Path;
use std::process::Command;

fn ruby(root: &Path, script: &str) -> (String, String) {
    let out = Command::new("ruby")
        .arg("-e")
        .arg(script)
        .current_dir(root)
        .output()
        .expect("ruby is on PATH");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

const ANCESTRY: &str = r#"puts Arel::BindError.ancestors.take_while { |m| m != Object }.map(&:name).inspect"#;

#[test]
fn bind_error_has_rails_arel_ancestry_and_is_rescued_as_an_arel_error() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (native, native_err) = ruby(
        root,
        &format!("begin; require \"active_support\"; require \"active_record\"; require \"arel\"; rescue LoadError; puts \"SKIP\"; exit 0; end\n{ANCESTRY}"),
    );
    if native.contains("SKIP") {
        prerequisite::missing("the activerecord gem is not installed");
        return;
    }
    assert!(native.contains("ArelError"), "native Rails arel failed\n{native}\n{native_err}");

    let (shim, shim_err) = ruby(
        root,
        &format!(
            r#"require_relative "runtime/ruby/active_record/arel"
{ANCESTRY}
begin
  Arel.sql("id = ?", 1, 2)
rescue Arel::ArelError
  puts "rescued as ArelError"
end"#
        ),
    );
    assert_eq!(
        shim.lines().next(),
        native.lines().next(),
        "shim ancestry differs from Rails'\n=== shim stderr ===\n{shim_err}"
    );
    assert!(shim.contains("rescued as ArelError"), "{shim}\n{shim_err}");
}

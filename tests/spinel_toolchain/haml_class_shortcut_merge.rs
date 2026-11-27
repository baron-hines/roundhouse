use super::haml_class_shortcut_merge::{app, render};

/// The page `tests/haml_class_shortcut_merge.rs` renders in CRuby, compiled
/// by Spinel: the truthy Array-literal merge.
#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn haml_class_merge_renders_the_same_page_natively() {
    let script = format!(
        "Db.configure(\":memory:\")\nSchema.statements.each {{ |sql| Db.exec(sql) }}\nActiveRecord.adapter = SqliteAdapter\n{}",
        render("1", "1")
    );
    let run = app().run_spinel(&script);
    run.assert_passes();
    assert!(
        run.stdout.contains("<div class=\"gadget gadget--hot gadget--cold\"></div>"),
        "got:\n{}",
        run.stdout
    );
}

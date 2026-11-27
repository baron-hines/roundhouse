//! The native half of `emit_and_run::active_model_serializer`: the same
//! serializers compile with Spinel and render the same JSON text.

use super::active_model_serializer_app as active_model_serializer;

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn an_active_model_serializer_renders_natively() {
    let run = active_model_serializer::overlay().run_spinel(&format!(
        "Db.configure(\":memory:\")\nSchema.statements.each {{ |sql| Db.exec(sql) }}\nActiveRecord.adapter = SqliteAdapter\n{}",
        active_model_serializer::ASSERTIONS
    ));
    run.assert_passes();
    assert!(run.stdout.contains("ActiveModel::Serializer OK"), "{}", run.stdout);
}

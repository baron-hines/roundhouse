#[path = "support/emit_and_run.rs"]
mod emit_and_run;

const CONTRACT: &str = include_str!("support/attribute_assignment_prepends/contract.rb");
const TEMPORAL_CONTRACT: &str = include_str!("support/attribute_assignment_prepends/temporal_contract.rb");

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\nend\n",
        )
        .write(
            "db/schema.rb",
            include_str!("support/attribute_assignment_prepends/schema.rb"),
        )
        .write(
            "app/models/attribute_record.rb",
            include_str!("support/attribute_assignment_prepends/model.rb"),
        )
        .write(
            "app/models/private_record.rb",
            include_str!("support/attribute_assignment_prepends/private_model.rb"),
        )
        .write(
            "app/channels/concerns/assignment_hooks.rb",
            include_str!("support/attribute_assignment_prepends/hooks.rb"),
        )
        .write(
            "app/channels/concerns/private_assignment_hooks.rb",
            include_str!("support/attribute_assignment_prepends/private_hooks.rb"),
        )
        .write(
            "config/initializers/assignment_hooks.rb",
            "AttributeRecord.prepend AssignmentHooks\nPrivateRecord.prepend PrivateAssignmentHooks\n",
        )
}

#[test]
fn initializer_temporal_prepend_receives_logical_values() {
    app().run_ruby(TEMPORAL_CONTRACT).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn initializer_temporal_prepend_runs_with_native_rbs() {
    let script = format!(
        "Db.configure(\":memory:\")\nSchema.statements.each {{ |sql| Db.exec(sql) }}\nActiveRecord.adapter = SqliteAdapter\n{TEMPORAL_CONTRACT}"
    );
    app().run_spinel_with_rbs(&script).assert_passes();
}

#[test]
fn initializer_prepends_keep_public_visibility_and_original_values() {
    app().run_ruby(CONTRACT).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn initializer_writer_prepends_run_with_native_rbs() {
    let script = format!(
        "Db.configure(\":memory:\")\nSchema.statements.each {{ |sql| Db.exec(sql) }}\nActiveRecord.adapter = SqliteAdapter\n{CONTRACT}"
    );
    app().run_spinel_with_rbs(&script).assert_passes();
}

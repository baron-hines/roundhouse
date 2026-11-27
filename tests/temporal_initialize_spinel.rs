#[path = "support/emit_and_run.rs"]
mod emit_and_run;

#[test]
#[ignore = "requires the Spinel toolchain"]
fn native_temporal_values_construct_before_entering_string_raw_slots() {
    let script = format!(
        "Db.configure(\":memory:\")\nSchema.statements.each {{ |sql| Db.exec(sql) }}\nActiveRecord.adapter = SqliteAdapter\n{}",
        include_str!("temporal_initialize/probe.rb"),
    );
    emit_and_run::empty_app()
        .write(
            "config/routes.rb",
            "Rails.application.routes.draw do\nend\n",
        )
        .write(
            "db/schema.rb",
            include_str!("temporal_initialize/schema.rb"),
        )
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n",
        )
        .write(
            "app/models/temporal_entry.rb",
            "class TemporalEntry < ApplicationRecord\nend\n",
        )
        .run_spinel_with_rbs(&script)
        .assert_passes();
}

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

const MODEL: &str = include_str!("support/class_lock/model.rb");
const CONTRACT: &str = include_str!("support/class_lock/contract.rb");

fn app(model: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table :lock_records do |t|\n    t.string :name\n  end\nend\n")
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\nend\n")
        .write("app/models/lock_record.rb", model)
}

#[test]
fn class_and_relation_lock_follow_sqlites_query_contract() {
    app(MODEL).run_ruby(CONTRACT).assert_passes();
}

#[test]
fn a_source_class_lock_keeps_its_implementation() {
    let model = MODEL.replace("class LockRecord < ApplicationRecord", "class LockRecord < ApplicationRecord\n  def self.lock(locking = true)\n    where(name: \"owned\")\n  end");
    app(&model).run_ruby("LockRecord.create!(name: \"owned\")\nLockRecord.create!(name: \"other\")\nraise \"override\" unless LockRecord.lock.count == 1\n").assert_passes();
}

#[test]
fn a_strict_target_reports_the_lock_runtime_boundary() {
    let (_, errors) = app(MODEL).emit(roundhouse::project::BuildTarget::Rust);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("locking relation")),
        "{errors:?}"
    );
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn class_and_relation_lock_run_with_native_rbs() {
    let script = format!(
        "Db.configure(\":memory:\")\nSchema.statements.each {{ |sql| Db.exec(sql) }}\nActiveRecord.adapter = SqliteAdapter\n{CONTRACT}"
    );
    app(MODEL).run_spinel_with_rbs(&script).assert_passes();
}

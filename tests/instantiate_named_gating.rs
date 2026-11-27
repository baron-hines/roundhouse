//! `ActiveRecord::Base.instantiate_named` gets its per-model `case` arms
//! only in an app that calls it (a `name.constantize.instantiate(attrs)`
//! snapshot rebuild). Spinel compiles the method either way, and with no
//! caller its untyped `attributes` reached every model's `instantiate`,
//! widening each `<Model>Row.from_raw` to the boxed path.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;

fn widgets(model: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n",
        )
        .write("app/models/widget.rb", model)
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"widgets\", force: :cascade do |t|\n    t.string \"name\"\n  end\nend\n",
        )
}

fn db_config(overlay: emit_and_run::Overlay) -> String {
    let (emitted, errors) = overlay.emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "emit errors: {errors:#?}");
    std::fs::read_to_string(emitted.join("runtime/active_record_db_config.rb")).expect("read db config")
}

#[test]
fn no_arms_without_a_caller() {
    let text = db_config(widgets("class Widget < ApplicationRecord\nend\n"));
    assert!(!text.contains("Widget.instantiate"), "arms emitted with no caller:\n{text}");
}

#[test]
fn arms_for_a_snapshot_rebuild() {
    let text = db_config(widgets(
        "class Widget < ApplicationRecord\n  def self.rebuild(name, attributes)\n    name.constantize.instantiate(attributes)\n  end\nend\n",
    ));
    assert!(text.contains("when \"Widget\" then Widget.instantiate(attributes)"), "no arm:\n{text}");
}

/// The caller scan skips only the runtime file that defines
/// `instantiate_named`, by its path in the emitted tree: an app file that
/// happens to share its basename is still a caller.
#[test]
fn arms_for_a_caller_sharing_the_runtime_basename() {
    let text = db_config(widgets("class Widget < ApplicationRecord\nend\n").write(
        "app/models/active_record_db_config.rb",
        "class ActiveRecordDbConfig\n  def self.rebuild(name, attributes)\n    name.constantize.instantiate(attributes)\n  end\nend\n",
    ));
    assert!(text.contains("when \"Widget\" then Widget.instantiate(attributes)"), "no arm:\n{text}");
}

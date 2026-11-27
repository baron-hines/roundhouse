//! A native temporal value must be formatted before entering its raw slot.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

fn app() -> emit_and_run::Overlay {
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
}

#[test]
fn constructors_never_send_native_values_to_a_string_raw_writer() {
    // CRuby enforces the same writer contract that Spinel's typed call does.
    let guard = r##"
guard = Module.new
[:happened_at, :required_at, :due_on, :opens_at].each do |column|
  guard.define_method("#{column}_raw=") do |value|
    raise TypeError, "#{column}: #{value.class} into String" unless value.nil? || value.is_a?(String)
    super(value)
  end
end
TemporalEntry.prepend(guard)
"##;
    let run = app().run_ruby(&format!(
        "{guard}\n{}",
        include_str!("temporal_initialize/probe.rb")
    ));
    run.assert_passes();
    assert!(
        run.stdout.contains("temporal construction passed"),
        "{}",
        run.stdout
    );
}

#[test]
fn temporal_construction_matches_rails() {
    let oracle = std::process::Command::new("ruby")
        .arg("tests/temporal_initialize/oracle.rb")
        .output()
        .expect("run Rails oracle");
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let run = app().run_ruby(include_str!("temporal_initialize/probe.rb"));
    run.assert_passes();
    assert_eq!(run.stdout, String::from_utf8_lossy(&oracle.stdout));
}

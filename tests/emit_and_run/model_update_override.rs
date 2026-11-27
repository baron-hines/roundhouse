//! A model's own `def update(params)` overrides ActiveRecord's `update`,
//! as an inherited method is overridden in Rails. Ignored as a fork spec
//! until the compiler does; the expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/profile.rb", r##"class Profile < ApplicationRecord
  def update(params)
    self.code = params[:code].presence
    save
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    profile = Profile.create!(code: "x", name: "first")
    answer = profile.update(code: "", name: "ignored")
    check("the own update answers", answer == true)
    profile = Profile.find(profile.id)
    # Rails: nil, "first"
    check("the own update maps a blank code", profile.code.nil?)
    check("the own update ignores the rest", profile.name == "first")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "profiles", force: :cascade do |t|
    t.string "code"
    t.string "name"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: persistence"]
fn a_models_own_update_overrides_active_record() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: persistence"]
fn a_models_own_update_overrides_active_record_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

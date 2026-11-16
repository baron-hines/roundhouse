//! `connection.quoted_true` and `quoted_false` answer the SQL literals
//! Rails 8.1's SQLite adapter writes, `TRUE` and `FALSE`, and a where
//! fragment built from them matches the stored booleans. Ignored as a
//! fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/switch.rb", r##"class Switch < ApplicationRecord
  def self.on_names
    where("enabled = #{connection.quoted_true}").order(:name).pluck(:name)
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    # Rails: "TRUE", "FALSE"
    check("quoted_true", Switch.connection.quoted_true == "TRUE")
    check("quoted_false", Switch.connection.quoted_false == "FALSE")
    Switch.create!(name: "b", enabled: true)
    Switch.create!(name: "a", enabled: false)
    Switch.create!(name: "c", enabled: true)
    # Rails: ["b", "c"]
    check("fragment", Switch.on_names == %w[b c])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "switches", force: :cascade do |t|
    t.string "name"
    t.boolean "enabled"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: record queries"]
fn quoted_true_and_false_answer_the_adapter_literals() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: record queries"]
fn quoted_true_and_false_answer_the_adapter_literals_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

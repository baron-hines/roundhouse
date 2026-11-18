//! A model whose declared primary key sits beside the table's
//! auto-increment `id` column keeps the key it was created with, and
//! `id` and `find` read that key, not the `id` column, as Rails' primary
//! key reads do. Ignored as a fork spec until the compiler does; the
//! expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/locker.rb", r##"class Locker < ApplicationRecord
  self.primary_key = "student_id"
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    d = Locker.create!(student_id: 42, name: "blue")
    # Rails: 42, 42, "blue"
    check("the given key is kept", d.student_id == 42)
    check("id reads the declared key", d.id == 42)
    check("find reads the declared key", Locker.find(42).name == "blue")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "lockers", force: :cascade do |t|
    t.integer "student_id"
    t.string "name"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: persistence"]
fn id_and_find_read_a_declared_primary_key() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: persistence"]
fn id_and_find_read_a_declared_primary_key_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

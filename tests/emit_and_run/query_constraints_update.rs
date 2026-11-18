//! A model declaring `query_constraints` addresses its UPDATE and DELETE
//! by every constraint column as the record was loaded, so assigning one
//! of them updates the row it came from and leaves a row with the same
//! `id` under another tenant alone, as Rails' `_query_constraints_hash`
//! does. Ignored as a fork spec until the compiler does; the expected
//! values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/locker.rb", r##"class Locker < ApplicationRecord
  query_constraints :site_id, :id
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    Locker.create!(id: 1, site_id: 10, label: "a")
    Locker.create!(id: 1, site_id: 20, label: "b")
    l = Locker.find_by!(site_id: 10, id: 1)
    l.site_id = 30
    l.label = "moved"
    l.save!
    # Rails: [[1, 20, "b"], [1, 30, "moved"]]
    rows = Locker.order(:site_id).pluck(:id, :site_id, :label)
    check("update addressed by the loaded constraints", rows == [[1, 20, "b"], [1, 30, "moved"]])
    l.destroy
    # Rails: [[1, 20, "b"]]
    check("delete addressed by the constraints", Locker.pluck(:id, :site_id, :label) == [[1, 20, "b"]])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "lockers", id: false, force: :cascade do |t|
    t.integer "id"
    t.integer "site_id"
    t.string "label"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: record queries"]
fn query_constraints_address_the_row_a_record_was_loaded_from() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: record queries"]
fn query_constraints_address_the_row_a_record_was_loaded_from_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

//! A `select` that leaves out a text primary key loads records whose `id`
//! is `nil`, not a value cast from a missing column, as Rails leaves the
//! attribute unread. Ignored as a fork spec until the compiler does; the
//! expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/voucher.rb", r##"class Voucher < ApplicationRecord
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    Voucher.create!(id: "v-100", label: "spring")
    Voucher.create!(id: "v-200", label: "autumn")
    picked = Voucher.select(:label).order(:label).to_a
    # Rails: ["autumn", "spring"], [nil, nil]
    check("selected column", picked.map(&:label) == %w[autumn spring])
    check("left-out key", picked.map(&:id) == [nil, nil])
    # Rails: "v-100"
    check("full load keeps the key", Voucher.find_by(label: "spring").id == "v-100")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "vouchers", id: :string, force: :cascade do |t|
    t.string "label"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: record queries"]
fn a_select_without_a_text_key_loads_a_nil_id() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: record queries"]
fn a_select_without_a_text_key_loads_a_nil_id_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

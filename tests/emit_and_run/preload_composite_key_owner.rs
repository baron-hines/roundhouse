//! An owner whose primary key is composite and includes `id` keys its
//! single-column associations on `id`: a has_many or has_one preload reads
//! the owner's id column, and a belongs_to preload finds the owner by it,
//! as Rails' `Preloader` reads `_read_attribute("id")`. Ingest refuses the
//! composite `self.primary_key` today, before any preload runs. Ignored as
//! a fork spec until the compiler does both; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/branch.rb", r##"class Branch < ApplicationRecord
  self.primary_key = [:bank_id, :id]
  has_many :tellers
  has_one :vault
end
"##)
        .write("app/models/teller.rb", r##"class Teller < ApplicationRecord
  belongs_to :branch
end
"##)
        .write("app/models/vault.rb", r##"class Vault < ApplicationRecord
  belongs_to :branch
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    Branch.create!(id: [1, 10], name: "high street")
    Branch.create!(id: [2, 20], name: "station")
    Teller.create!(name: "kim", branch_id: 10)
    Teller.create!(name: "lee", branch_id: 20)
    Teller.create!(name: "max", branch_id: 20)
    Vault.create!(size: 3, branch_id: 20)
    branches = Branch.order(:id).preload(:tellers, :vault).to_a
    # Rails: keyed on the id column
    check("preloaded", branches.all? { |b| b.association(:tellers).loaded? && b.association(:vault).loaded? })
    check("has_many", branches.map { |b| b.tellers.map(&:name).sort } == [["kim"], ["lee", "max"]])
    check("has_one", branches.map { |b| b.vault && b.vault.size } == [nil, 3])
    tellers = Teller.order(:name).preload(:branch).to_a
    check("belongs_to preloaded", tellers.all? { |t| t.association(:branch).loaded? })
    check("belongs_to", tellers.map { |t| t.branch.name } == ["high street", "station", "station"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "branches", primary_key: ["bank_id", "id"], force: :cascade do |t|
    t.integer "bank_id"
    t.integer "id"
    t.string "name"
  end
  create_table "tellers", force: :cascade do |t|
    t.string "name"
    t.integer "branch_id"
  end
  create_table "vaults", force: :cascade do |t|
    t.integer "size"
    t.integer "branch_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: preload"]
fn a_composite_key_owner_preloads_on_its_id_column() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: preload"]
fn a_composite_key_owner_preloads_on_its_id_column_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

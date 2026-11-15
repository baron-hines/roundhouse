//! Joining a scoped `belongs_to` puts its scope in the ON clause over the
//! target's default scope, as Rails' association join does: a `where`
//! adds its condition, an `unscope(where:)` lifts the default's, and
//! `includes` or `order` change nothing. The expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/vendor.rb", r##"class Vendor < ApplicationRecord
  default_scope { where(closed: false) }
end
"##)
        .write("app/models/region.rb", r##"class Region < ApplicationRecord
  has_many :invoices
end
"##)
        .write("app/models/invoice.rb", r##"class Invoice < ApplicationRecord
  belongs_to :vendor, -> { unscope(where: :closed) }
  belongs_to :approved_vendor, -> { where(approved: true) }, class_name: "Vendor", foreign_key: :vendor_id, optional: true
  belongs_to :region, -> { where(active: true).includes(:invoices).order(:name) }, optional: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    north = Region.create!(name: "north", active: true)
    south = Region.create!(name: "south", active: false)
    open_ok = Vendor.create!(name: "open approved", approved: true)
    open_new = Vendor.create!(name: "open new", approved: false)
    shut = Vendor.create!(name: "shut approved", approved: true, closed: true)
    Invoice.create!(number: 1, vendor_id: open_ok.id, region: north)
    Invoice.create!(number: 2, vendor_id: open_new.id, region: south)
    Invoice.create!(number: 3, vendor_id: shut.id, region: north)
    # Rails: [1, 2, 3]
    check("unscope", Invoice.joins(:vendor).order(:number).pluck(:number) == [1, 2, 3])
    # Rails: [1]
    check("where over the default scope", Invoice.joins(:approved_vendor).order(:number).pluck(:number) == [1])
    # Rails: [1, 3]
    check("where", Invoice.joins(:region).order(:number).pluck(:number) == [1, 3])
    # Rails: [[1, "north"], [2, nil], [3, "north"]]
    check("left join", Invoice.left_outer_joins(:region).order(:number).pluck(:number, "regions.name") == [[1, "north"], [2, nil], [3, "north"]])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "vendors", force: :cascade do |t|
    t.string "name"
    t.boolean "approved", default: false
    t.boolean "closed", default: false
  end
  create_table "regions", force: :cascade do |t|
    t.string "name"
    t.boolean "active", default: true
  end
  create_table "invoices", force: :cascade do |t|
    t.integer "number"
    t.integer "vendor_id"
    t.integer "region_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_belongs_to_join_applies_its_scope() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_belongs_to_join_applies_its_scope_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

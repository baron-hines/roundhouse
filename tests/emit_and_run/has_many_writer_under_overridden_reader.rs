//! A has_many writer replaces the association's records even when a
//! subclass overrides the association's reader: `owner.items = list`
//! goes through the association, not the reader, as Rails' generated
//! writer does. Ignored as a fork spec until the compiler does; the
//! expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/route.rb", r##"class Route < ApplicationRecord
  has_many :stops
end
"##)
        .write("app/models/express_route.rb", r##"class ExpressRoute < Route
  def stops
    super.where(major: true)
  end
end
"##)
        .write("app/models/stop.rb", r##"class Stop < ApplicationRecord
  belongs_to :route, optional: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    r = ExpressRoute.create!(name: "x1")
    old = Stop.create!(name: "depot", major: true, route: r)
    a = Stop.create!(name: "harbor", major: true)
    b = Stop.create!(name: "mill", major: false)
    r.stops = [a, b]
    # Rails: the old stop leaves, both new stops join, the reader filters
    check("rows replaced", Stop.where(route_id: r.id).order(:name).pluck(:name) == ["harbor", "mill"])
    check("old stop nulled", old.reload.route_id.nil?)
    check("reader still filters", ExpressRoute.find(r.id).stops.map(&:name) == ["harbor"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "routes", force: :cascade do |t|
    t.string "name"
    t.string "type"
  end
  create_table "stops", force: :cascade do |t|
    t.string "name"
    t.boolean "major", default: false
    t.integer "route_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: association targets"]
fn a_has_many_writer_replaces_records_under_an_overridden_reader() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: association targets"]
fn a_has_many_writer_replaces_records_under_an_overridden_reader_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

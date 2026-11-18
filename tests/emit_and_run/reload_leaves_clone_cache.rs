//! A reloaded record starts its association cache again, so it reads
//! what the database holds now, while its clone keeps the rows it loaded
//! before, as Rails' `reload` replaces `@association_cache`. Ignored as
//! a fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/shelf.rb", r##"class Shelf < ApplicationRecord
  has_many :jars
  has_one :label
end
"##)
        .write("app/models/jar.rb", r##"class Jar < ApplicationRecord
  belongs_to :shelf, optional: true
end
"##)
        .write("app/models/label.rb", r##"class Label < ApplicationRecord
  belongs_to :shelf, optional: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    s = Shelf.create!(name: "top")
    Jar.create!(shelf_id: s.id, name: "jam")
    old_label = Label.create!(shelf_id: s.id, text: "old")
    original = Shelf.find(s.id)
    copy = original.clone
    check("the clone loads the jars", copy.jars.map(&:name) == ["jam"])
    check("the clone loads the label", copy.label.text == "old")
    Jar.create!(shelf_id: s.id, name: "honey")
    old_label.destroy
    Label.create!(shelf_id: s.id, text: "new")
    original.reload
    # Rails: the reloaded original reads the rows now; the clone keeps its own
    check("the reloaded record reads the new jars", original.jars.map(&:name).sort == ["honey", "jam"])
    check("the reloaded record reads the new label", original.label.text == "new")
    check("the clone keeps its jars", copy.jars.map(&:name) == ["jam"])
    check("the clone keeps its label", copy.label.text == "old")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "shelves", force: :cascade do |t|
    t.string "name"
  end
  create_table "jars", force: :cascade do |t|
    t.integer "shelf_id"
    t.string "name"
  end
  create_table "labels", force: :cascade do |t|
    t.integer "shelf_id"
    t.string "text"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: persistence"]
fn a_reloaded_record_leaves_its_clones_cache() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: persistence"]
fn a_reloaded_record_leaves_its_clones_cache_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

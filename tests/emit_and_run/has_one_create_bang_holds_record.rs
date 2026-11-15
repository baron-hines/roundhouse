//! A has_one's `create_<name>!` saves the new record once, not once for
//! the build and again for the replace, and the owner then holds that very
//! record, as Rails' `set_new_record` hands it to the association. The
//! single save already holds on this compiler; the owner holding the
//! record does not, so this is ignored as a fork spec until it does. The
//! expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/lighthouse.rb", r##"class Lighthouse < ApplicationRecord
  has_one :keeper
end
"##)
        .write("app/models/keeper.rb", r##"class Keeper < ApplicationRecord
  belongs_to :lighthouse

  before_save do
    self.saves = saves + 1
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    l = Lighthouse.create!(name: "cape")
    k = l.create_keeper!(name: "ada")
    # Rails: one save, and the owner holds the created record
    check("in memory", k.saves == 1)
    check("in the row", Keeper.find(k.id).saves == 1)
    check("held", l.keeper.equal?(k))
    k.name = "grace"
    check("held edits", l.keeper.name == "grace")
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "lighthouses", force: :cascade do |t|
    t.string "name"
  end
  create_table "keepers", force: :cascade do |t|
    t.string "name"
    t.integer "saves", default: 0
    t.integer "lighthouse_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: association targets"]
fn a_has_one_create_bang_saves_once_and_holds_the_record() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: association targets"]
fn a_has_one_create_bang_saves_once_and_holds_the_record_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

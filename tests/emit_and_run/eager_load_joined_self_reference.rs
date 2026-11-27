//! An `eager_load` over a `joins` of the same self-referencing path
//! reads each nested record from its own aliased JOIN, not from the base
//! table's row, as Rails' JoinDependency does. Ignored as a fork spec
//! until the compiler does; the expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/category.rb", r##"class Category < ApplicationRecord
  has_many :links, foreign_key: :child_id
end
"##)
        .write("app/models/link.rb", r##"class Link < ApplicationRecord
  belongs_to :child, class_name: "Category"
  belongs_to :parent, class_name: "Category"
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    root = Category.create!(name: "root")
    tools = Category.create!(name: "tools")
    saws = Category.create!(name: "saws")
    Link.create!(child: tools, parent: root)
    Link.create!(child: saws, parent: tools)
    rows = Category.joins(links: [:parent]).eager_load(links: [:parent]).order(:id).to_a
    # Rails: [["tools", ["root"]], ["saws", ["tools"]]]
    read = rows.map { |r| [r.name, r.links.map { |l| l.parent.name }] }
    check("each parent from its own JOIN", read == [["tools", ["root"]], ["saws", ["tools"]]])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "categories", force: :cascade do |t|
    t.string "name"
  end
  create_table "links", force: :cascade do |t|
    t.integer "child_id"
    t.integer "parent_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: relation queries"]
fn an_eager_load_over_a_joined_self_reference_reads_its_alias() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: relation queries"]
fn an_eager_load_over_a_joined_self_reference_reads_its_alias_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

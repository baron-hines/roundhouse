//! `joins(:assoc)` of a has_many / has_one carries that association's
//! own scope in the join's ON clause, as Rails' association join does, so
//! `Orchard.joins(:tall_trees)` reaches only the tall trees. The expected
//! values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/orchard.rb", r##"class Orchard < ApplicationRecord
  has_many :trees
  has_many :tall_trees, -> { where(tall: true) }, class_name: "Tree"
  has_one :featured_tree, -> { where(featured: true) }, class_name: "Tree"
end
"##)
        .write("app/models/tree.rb", r##"class Tree < ApplicationRecord
  belongs_to :orchard
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    north = Orchard.create!(name: "north")
    south = Orchard.create!(name: "south")
    Orchard.create!(name: "east")
    Tree.create!(name: "fir", tall: true, featured: true, orchard: north)
    Tree.create!(name: "oak", tall: false, featured: false, orchard: north)
    Tree.create!(name: "pine", tall: true, featured: false, orchard: south)
    Tree.create!(name: "elm", tall: false, featured: false, orchard: south)
    # Rails: the scoped association's predicate is in the join, so only
    # orchards with a tall tree are rows.
    check("has_many count", Orchard.joins(:tall_trees).count == 2)
    check("has_many pluck", Orchard.joins(:tall_trees).pluck(:name).sort == ["north", "south"])
    check("has_many where", Orchard.joins(:tall_trees).where(name: "north").pluck(:name) == ["north"])
    # Rails: the has_one's scope narrows its join the same way.
    check("has_one count", Orchard.joins(:featured_tree).count == 1)
    check("has_one pluck", Orchard.joins(:featured_tree).pluck(:name) == ["north"])
    check("has_one where", Orchard.joins(:featured_tree).where(name: "south").pluck(:name) == [])
    # Rails: in an outer join the scope stays in ON, so every orchard is a
    # row and only matching trees join it.
    check("has_many left join", Orchard.left_joins(:tall_trees).pluck(:name).sort == ["east", "north", "south"])
    check("has_one left join", Orchard.left_joins(:featured_tree).pluck(:name).sort == ["east", "north", "south"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "orchards", force: :cascade do |t|
    t.string "name"
  end
  create_table "trees", force: :cascade do |t|
    t.string "name"
    t.boolean "tall"
    t.boolean "featured"
    t.integer "orchard_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn joins_of_a_scoped_has_many_or_has_one_applies_its_scope() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn joins_of_a_scoped_has_many_or_has_one_applies_its_scope_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

/// A join whose association scope renders no ON clause here (a SQL
/// string, `where.not`, an enum Symbol) is refused by `check`, where it
/// is written, rather than left to raise at run time; the probe's own
/// hash scopes are not.
#[test]
fn a_join_whose_scope_does_not_render_is_a_check_error() {
    let errors = app()
        .edit(
            "app/models/orchard.rb",
            "  has_many :trees\n",
            "  has_many :trees\n  has_many :shady_trees, -> { where(\"tall = ?\", false) }, class_name: \"Tree\"\n  has_one :plain_tree, -> { where.not(featured: true) }, class_name: \"Tree\"\n  scope :shady, -> { joins(:shady_trees) }\n  scope :shady_north, -> { where(name: \"north\").joins(:shady_trees) }\n\n  def self.plain\n    self.left_joins(:plain_tree)\n  end\n",
        )
        .write(
            "app/models/report.rb",
            "class Report\n  def self.shady\n    Orchard.joins(:shady_trees).count\n  end\n\n  def self.plain\n    Orchard.where(name: \"north\").left_joins(:plain_tree, :tall_trees).count\n  end\n\n  def self.nested\n    Tree.joins(orchard: :shady_trees).count\n  end\nend\n",
        )
        .check_errors();
    assert_eq!(errors.len(), 6, "{errors:#?}");
    assert!(errors[0].contains("`joins(:shady_trees)`") || errors[1].contains("`joins(:shady_trees)`"), "{errors:#?}");
    // A bare join in a scope and a `self.` one in a class method are on
    // their model.
    assert_eq!(errors.iter().filter(|e| e.contains("`left_joins(:plain_tree)`")).count(), 2, "{errors:#?}");
    // A nested association is checked on the preceding one's target.
    assert_eq!(errors.iter().filter(|e| e.contains("`joins(:shady_trees)`")).count(), 4, "{errors:#?}");
}

//! A bare `unscope(:where)` or `find` in an association-extension
//! method is relation surface, like `where`: it has to reach the
//! association, not the owner.
//!
//! Rails 8.1.4: `unscope(:where)` drops every condition of the
//! association, the owner's foreign key included, so `inactive` below
//! answers the inactive widgets of every shop. `find` stays in the
//! association. The Spinel side is
//! `support/small_rails_gaps_app.rs`.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const APPLICATION_RECORD: &str = "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n";

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"shops\", force: :cascade do |t|\n    t.string \"name\"\n  end\n  create_table \"widgets\", force: :cascade do |t|\n    t.integer \"shop_id\"\n    t.string \"name\"\n    t.boolean \"active\", default: true, null: false\n  end\nend\n",
        )
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write("app/models/application_record.rb", APPLICATION_RECORD)
        .write("app/models/widget.rb", "class Widget < ApplicationRecord\n  belongs_to :shop, optional: true\nend\n")
        .write(
            "app/models/shop.rb",
            r#"class Shop < ApplicationRecord
  has_many :widgets, -> { where(active: true) } do
    def inactive = unscope(:where).where(active: false)
    def lookup(id) = find(id)
    def names_only = select(:name)
  end

  def inactive_names
    self.widgets.inactive.order(:name).map { |w| w.name }
  end

  def only_names
    self.widgets.names_only.map { |w| w.name }
  end

  def lookup_name(id)
    self.widgets.lookup(id).name
  end
end
"#,
        )
}

const SCRIPT: &str = r#"
s = Shop.create!(name: "s")
t = Shop.create!(name: "t")
a = Widget.create!(shop_id: s.id, name: "a")
Widget.create!(shop_id: s.id, name: "b", active: false)
Widget.create!(shop_id: t.id, name: "c")
d = Widget.create!(shop_id: t.id, name: "d", active: false)
got = s.inactive_names
raise "inactive: #{got.inspect}" unless got == ["b", "d"]
got = s.only_names
raise "select: #{got.inspect}" unless got == ["a"]
raise "find: #{s.lookup_name(a.id)}" unless s.lookup_name(a.id) == "a"
begin
  s.lookup_name(d.id)
  raise "find crossed the association"
rescue ActiveRecord::RecordNotFound
end
puts "extension unscope passed"
"#;

#[test]
fn bare_unscope_and_find_in_an_extension_reach_the_association() {
    app().run_ruby(SCRIPT).assert_passes();
}

/// `Relation#find` takes one id. `find { }` is enumeration of the
/// records, so a bare block form is not relation surface to re-root.
#[test]
fn a_bare_find_with_a_block_is_not_rerooted() {
    for target in [roundhouse::project::BuildTarget::Ruby, roundhouse::project::BuildTarget::Spinel] {
        let (emitted, _) = app()
            .edit(
                "app/models/shop.rb",
                "    def names_only = select(:name)\n",
                "    def names_only = select(:name)\n    def first_match = find { |w| w.name == \"a\" }\n",
            )
            .edit(
                "app/models/shop.rb",
                "  def only_names\n",
                "  def matched\n    self.widgets.first_match\n  end\n\n  def only_names\n",
            )
            .emit(target);
        let source = std::fs::read_to_string(emitted.join("app/models/shop.rb")).expect("model");
        assert!(source.contains("def widgets_first_match"), "{source}");
        assert!(
            !source.contains("widgets.find {") && !source.contains("widgets.find do"),
            "the block form stays where it was written:\n{source}"
        );
    }
}

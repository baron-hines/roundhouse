use super::emit_and_run;

const MEMBER: &str = r#"class Member < ApplicationRecord
  has_many :children, class_name: "Member", foreign_key: :parent_id
end
"#;

const REPORT: &str = r#"class ChildReport
  include ActiveModel::Model

  def rows(member)
    details(member.children)
  end

  def relation_rows(member)
    details(Member.where(parent_id: member.id))
  end

  def looked_up_rows(id)
    member = Member.find(id)
    details(member.children)
  end

  def forwarded_rows(member)
    forward(member.children)
  end

  def class_rows(member)
    ChildQueries.details(member.children)
  end

  def forward(children)
    details(children)
  end

  def selected_rows(member)
    selected(member.children)
  end

  def selected(children)
    children.where(name: "Alpha").order(:id).pluck(:id, :name, :parent_id)
  end

  def cached_rows(member)
    cached_details(member.children)
  end

  def cached_details(children)
    children.order(:id).map { |child| [child.id, child.name, child.parent_id] }
  end

  private

  def details(children)
    children.order(:id).pluck(:id, :name, :parent_id)
  end
end
"#;

const RELATION_METHOD: &str =
    "  def relation_rows(member)\n    details(Member.where(parent_id: member.id))\n  end\n\n";

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "db/schema.rb",
            r#"ActiveRecord::Schema.define(version: 1) do
  create_table :members do |t|
    t.string :name
    t.integer :parent_id
  end
end
"#,
        )
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n",
        )
        .write("app/models/member.rb", MEMBER)
        .write("app/models/child_report.rb", REPORT)
        .write(
            "app/models/child_queries.rb",
            r#"class ChildQueries
  def self.details(children)
    children.order(:id).pluck(:id, :name, :parent_id)
  end

  def raw_rows(member)
    details(member.children)
  end

  def details(children)
    children
  end
end
"#,
        )
}

const SETUP: &str = r#"
member = Member.create!(name: "Parent")
first = Member.create!(name: "Alpha", parent_id: member.id)
second = Member.create!(name: "Beta", parent_id: member.id)
other = Member.create!(name: "Other parent")
Member.create!(name: "Alpha", parent_id: other.id)
report = ChildReport.new
expected = [[first.id, "Alpha", member.id], [second.id, "Beta", member.id]]
"#;

const CHECK_ASSOCIATION: &str = r#"
raise report.rows(member).inspect unless report.rows(member) == expected
raise report.forwarded_rows(member).inspect unless report.forwarded_rows(member) == expected
raise report.class_rows(member).inspect unless report.class_rows(member) == expected
raise report.looked_up_rows(member.id).inspect unless report.looked_up_rows(member.id) == expected
raise report.selected_rows(member).inspect unless report.selected_rows(member) == [[first.id, "Alpha", member.id]]
puts "association query arguments passed"
"#;

#[test]
fn a_helper_queries_both_an_association_argument_and_a_relation_argument() {
    app()
        .run_ruby(&format!(
            "{SETUP}{CHECK_ASSOCIATION}\nraise unless report.relation_rows(member) == expected\n"
        ))
        .assert_passes();
}

#[test]
fn helper_query_arguments_open_the_scope_free_rewrite_gate() {
    app()
        .write(
            "app/models/child_report.rb",
            &REPORT.replace(RELATION_METHOD, ""),
        )
        .run_ruby(&format!("{SETUP}{CHECK_ASSOCIATION}"))
        .assert_passes();
}

#[test]
fn helper_queries_keep_preloaded_rows_and_do_not_change_other_helpers() {
    app()
        .write(
            "app/models/array_report.rb",
            r#"class ArrayReport
  def rows(member)
    details(member.children)
  end

  def details(children)
    children
  end
end
"#,
        )
        .run_ruby(&format!(
            r#"{SETUP}
member._preload_children([second])
raise report.cached_rows(member).inspect unless report.cached_rows(member) == [[second.id, "Beta", member.id]]
raise report.rows(member).inspect unless report.rows(member) == expected
raw = member.children
raise raw.inspect unless raw.is_a?(Array) && raw == [second]
raise unless ArrayReport.new.rows(member).equal?(raw)
raise unless ChildQueries.new.raw_rows(member).equal?(raw)
puts "preloaded query arguments and reader arrays passed"
"#
        ))
        .assert_passes();
}

#[test]
fn a_query_on_a_shadowed_or_reassigned_name_does_not_retype_the_argument() {
    app()
        .write(
            "app/models/array_report.rb",
            r#"class ArrayReport
  def rows(member)
    shadowed(member.children)
  end

  def shadowed(children)
    [ActiveRecord::Relation.new(Member)].each { |children| children.order(:id) }
    children
  end

  def reassigned(member)
    replaced(member.children)
  end

  def replaced(children)
    original = children
    children = ActiveRecord::Relation.new(Member)
    children.order(:id)
    original
  end
end
"#,
        )
        .run_ruby(&format!(
            r#"{SETUP}
member._preload_children([second])
raw = member.children
raise unless ArrayReport.new.rows(member).equal?(raw)
raise unless ArrayReport.new.reassigned(member).equal?(raw)
puts "unrelated helper bindings passed"
"#
        ))
        .assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn spinel_gate_association_query_arguments() {
    app()
        .run_spinel_with_db(&format!(
            "{SETUP}{CHECK_ASSOCIATION}\nraise unless report.relation_rows(member) == expected\n"
        ))
        .assert_passes();
}

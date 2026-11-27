//! The Hash forms of `order` and `where` quote the columns they write,
//! as Rails does. A column named after an SQL keyword (`index`,
//! `order`) was written bare by the runtime relation:
//! `ORDER BY index ASC` and `WHERE (gadgets.index = ?)` are SQLite
//! syntax errors. `pluck` and the calculations already quoted.
//!
//! The expected answers are Rails 8.1's on the same schema and rows.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema[8.1].define(version: 1) do\n  create_table \"gadgets\", force: :cascade do |t|\n    t.string \"name\"\n    t.integer \"index\"\n    t.integer \"order\"\n  end\nend\n",
        )
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n")
        .write(
            "app/models/gadget.rb",
            r#"class Gadget < ApplicationRecord
  def self.by_position
    order(index: :asc)
  end

  def self.at_position(n)
    where(index: n)
  end

  def self.not_at_position(n)
    where.not(index: n)
  end

  def self.by_order_column
    order(order: :desc)
  end

  def self.by_dotted_position
    order("gadgets.index" => :desc)
  end

  def self.by_qualified_position
    order(gadgets: { index: :desc })
  end

  def self.at_qualified_position(n)
    where(gadgets: { index: n })
  end
end
"#,
        )
}

#[test]
fn hash_order_and_where_quote_keyword_columns() {
    app()
        .run_ruby(
            r##"
Gadget.create!(name: "b", index: 2, order: 1)
Gadget.create!(name: "a", index: 1, order: 2)
expected = {
  "order(index: :asc)" => [-> { Gadget.by_position.map(&:name) }, ["a", "b"]],
  "where(index: 1)" => [-> { Gadget.at_position(1).map(&:name) }, ["a"]],
  "where.not(index: 1)" => [-> { Gadget.not_at_position(1).map(&:name) }, ["b"]],
  "find_by(index: 2)" => [-> { Gadget.find_by(index: 2)&.name }, "b"],
  "order(order: :desc)" => [-> { Gadget.by_order_column.map(&:name) }, ["a", "b"]],
  "order(\"gadgets.index\" => :desc)" => [-> { Gadget.by_dotted_position.map(&:name) }, ["b", "a"]],
  "order(gadgets: { index: :desc })" => [-> { Gadget.by_qualified_position.map(&:name) }, ["b", "a"]],
  "where(gadgets: { index: 2 })" => [-> { Gadget.at_qualified_position(2).map(&:name) }, ["b"]],
}
failures = expected.filter_map do |label, (call, want)|
  got = begin
    call.call
  rescue => e
    "#{e.class}: #{e.message}"
  end
  "#{label}: expected #{want.inspect}, got #{got.inspect}" unless got == want
end
# A loaded relation re-sorts in memory rather than re-querying: the
# quoted term must still read back as a bare column.
loaded = Gadget.all
loaded.to_a
resorted = loaded.order(index: :desc)
failures << "order(index:) on a loaded relation re-queried" if resorted.instance_variable_get(:@records).nil?
failures << "order(index: :desc) on a loaded relation: #{resorted.map(&:name).inspect}" unless resorted.map(&:name) == ["b", "a"]
raise failures.join("\n") unless failures.empty?
puts "ALL OK"
"##,
        )
        .assert_passes();
}

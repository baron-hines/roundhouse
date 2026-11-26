//! A concern's class-side macro that walks its `*rest` and defines a method per name with a block of its own
//! parameters (maybe's `Monetizable#monetize`: `fields.each do |field| define_method("#{field}_money") do |**args|
//! … end end`), called by the model and by a concern it includes. Ingest refused it ("model macro not expanded");
//! the call is now unrolled over its literal names, each method taking the block's parameters, `send(field, **args)`
//! the reader called by name.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const CONCERN: &str = r##"module Pricey
  extend ActiveSupport::Concern

  class_methods do
    def pricey(*fields)
      fields.each do |field|
        define_method("#{field}_label") do |**args|
          value = self.send(field, **args)

          return nil if value.nil? || label_unit.nil?

          Label.new(value, label_unit)
        end
      end
    end
  end

  private
    def label_unit
      "pts"
    end
end
"##;

const EXTRA: &str = r#"module Ranked
  extend ActiveSupport::Concern

  included do
    pricey :rank
  end
end
"#;

const LABEL: &str = "class Label\n  def initialize(value, unit)\n    @value = value\n    @unit = unit\n  end\n\n  def to_s\n    \"#{@value} #{@unit}\"\n  end\nend\n";

const COLUMNS: &str = "create_table \"articles\", force: :cascade do |t|\n    t.integer \"price\"\n    t.integer \"rank\"";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("db/schema.rb", "create_table \"articles\", force: :cascade do |t|", COLUMNS)
        .write("app/models/concerns/pricey.rb", CONCERN)
        .write("app/models/concerns/ranked.rb", EXTRA)
        .write("app/models/label.rb", LABEL)
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  include Pricey\n  include Ranked\n  pricey :price\n",
        )
}

const SCRIPT: &str = r##"
a = Article.new(title: "Priced", body: "A body long enough.", price: 5, rank: 2)
puts a.price_label.to_s
puts a.rank_label.to_s
puts Article.new(title: "Unpriced", body: "A body long enough.").price_label.inspect
"##;

const EXPECTED: &str = "5 pts\n2 pts\nnil\n";

#[test]
fn a_rest_walking_macro_defines_each_method_on_ruby() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert_eq!(run.stdout, EXPECTED);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_rest_walking_macro_defines_each_method_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert_eq!(run.stdout, EXPECTED);
}

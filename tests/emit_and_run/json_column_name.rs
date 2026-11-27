//! An app's own top-level `JsonColumn` (a common name for a `serialize`
//! coder) is not the runtime's JSON column codec. The compiled accessors
//! of a JSON column called that codec by the bare name `JsonColumn`, so a
//! writer reached the app's class (`undefined method 'dump' for class
//! JsonColumn`), and the runtime `module JsonColumn` loaded first made the
//! app's `class JsonColumn` fail to load. The runtime module is
//! `RoundhouseJsonColumn`.

use super::emit_and_run;

const APP_CODER: &str = r#"class JsonColumn
  def dump(value)
    "app:#{value.inspect}"
  end

  def load(text)
    text
  end
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "create_table \"articles\", force: :cascade do |t|",
            "create_table \"articles\", force: :cascade do |t|\n    t.json \"metadata\"",
        )
        .write("app/models/json_column.rb", APP_CODER)
}

const ASSERTIONS: &str = r##"raise "app JsonColumn replaced" unless JsonColumn.is_a?(Class) && JsonColumn.new.dump(1) == "app:1"
value = [{ "name" => "Ada", "enabled" => true }]
article = Article.create!(title: "JSON", body: "A body long enough to validate.", metadata: value)
article.metadata = { "name" => "Grace" }
article.save!
got = [Article.find(article.id).metadata, Article.find(article.id)[:metadata]]
raise got.inspect unless got == [{ "name" => "Grace" }, { "name" => "Grace" }]
puts "json column name passed"
"##;

#[test]
fn a_json_column_writer_works_beside_an_app_class_named_json_column() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("json column name passed"), "{}", run.stdout);
}

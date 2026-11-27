//! maybe's `Rule::Registry`: the base's `def condition_filters = []` is overridden by
//! `TransactionResource`'s list of filters (of several classes, `[TransactionName.new(rule), …]`), and the base's `as_json` maps them,
//! `condition_filters.map(&:as_json)`. Ruby dispatches the call to the subclass, so
//! the list is the subclass's filters, each answering its own `as_json`.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "app/models/registry.rb",
            "class Registry\n  def filters\n    []\n  end\n\n  def as_json\n    { filters: filters.map(&:as_json) }\n  end\nend\n",
        )
        .write(
            "app/models/registry/article_resource.rb",
            "class Registry::ArticleResource < Registry\n  def filters\n    [TitleFilter.new(\"title\"), BodyFilter.new(\"body\")]\n  end\nend\n",
        )
        .write("app/models/body_filter.rb", "class BodyFilter < TitleFilter\nend\n")
        .write(
            "app/models/title_filter.rb",
            "class TitleFilter\n  def initialize(key)\n    @key = key\n  end\n\n  def as_json\n    { key: @key, label: @key.capitalize }\n  end\nend\n",
        )
}

const ASSERTIONS: &str = r##"
got = Registry::ArticleResource.new.as_json
raise got.inspect unless got == { filters: [{ key: "title", label: "Title" }, { key: "body", label: "Body" }] }
raise "base" unless Registry.new.as_json == { filters: [] }
puts "base list override passed"
"##;

#[test]
fn a_base_lists_override_answers_for_the_subclass() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("base list override passed"), "{}", run.stdout);
}

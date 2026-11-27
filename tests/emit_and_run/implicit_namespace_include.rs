//! maybe's `class PlaidAccount::Transactions::CategoryMatcher; include PlaidAccount::Transactions::CategoryTaxonomy`:
//! a class under a namespace only the directory names includes a module beside it, which Rails autoloads; the
//! emitted file requires the module's file before the include runs, also when a top-level module of the namespace's
//! name exists too (maybe's `Transactions::BulkUpdatesController`).

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/controllers/parts/probes_controller.rb", "class Parts::ProbesController < ApplicationController\nend\n")
        .write("app/models/article/parts/taxonomy.rb", "module Article::Parts::Taxonomy\n  NAMES = %w[a b].freeze\nend\n")
        .write(
            "app/models/article/parts/matcher.rb",
            "class Article::Parts::Matcher\n  include Article::Parts::Taxonomy\n\n  def names\n    NAMES.join(\",\")\n  end\nend\n",
        )
}

const SCRIPT: &str = r#"
got = Article::Parts::Matcher.new.names
raise "got #{got.inspect}" unless got == "a,b"
puts "namespace include passed"
"#;

#[test]
fn an_include_beside_a_class_under_an_implicit_namespace_loads_first() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("namespace include passed"), "{}", run.stdout);
}

//! `ActiveRecord::Base.with_connection` holds one connection for its block
//! and answers the block's value; `prevent_permanent_checkout:` only
//! changes how the pool keeps a connection leased inside the block, which
//! this runtime's single slot does not pool (Rails 8.1).

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit(
        "app/models/article.rb",
        "class Article < ApplicationRecord\n",
        "class Article < ApplicationRecord\n  def self.titles_held\n    ActiveRecord::Base.with_connection(prevent_permanent_checkout: true) do\n      Article.order(:id).pluck(:title)\n    end\n  end\n\n  def self.count_on(sql)\n    ActiveRecord::Base.with_connection { |connection| connection.select_value(sql) }\n  end\n\n",
    )
}

#[test]
fn base_with_connection_runs_its_block() {
    let errors = app().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
    app()
        .run_ruby("Article.create!(title: \"Held\", body: \"A long enough body.\")\ngot = [Article.titles_held, Article.count_on(\"SELECT COUNT(*) FROM articles\").to_i]\nraise got.inspect unless got == [[\"Held\"], 1]\nputs \"with_connection passed\"\n")
        .assert_passes();
}


#[test]
#[ignore = "requires the Spinel toolchain"]
fn base_with_connection_runs_its_block_on_spinel() {
    app()
        .run_spinel_with_db("Article.create!(title: \"Held\", body: \"A long enough body.\")\ngot = [Article.titles_held, Article.count_on(\"SELECT COUNT(*) FROM articles\").to_i]\nraise got.inspect unless got == [[\"Held\"], 1]\nputs \"with_connection passed\"\n")
        .assert_passes();
}


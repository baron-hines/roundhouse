//! `enum :state, (BASE.dup << %i[archived]).flatten` is a mapping Rails
//! evaluates when the class body runs, and every input is a frozen
//! literal. Ingest folds `dup`, `<<`, `+` and `flatten` over such lists;
//! any other computed mapping stays refused.
//!
//! Rails 8.1.4: `Widget.states` is
//! `{"draft" => 0, "live" => 1, "archived" => 2}`. `BASE << :x` raises
//! `FrozenError`, so it is refused rather than folded.
//! The Spinel side is `support/small_rails_gaps_app.rs`.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const APPLICATION_RECORD: &str = "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n";

fn app(mapping: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"widgets\", force: :cascade do |t|\n    t.integer \"state\", default: 0, null: false\n  end\nend\n",
        )
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write("app/models/application_record.rb", APPLICATION_RECORD)
        .write(
            "app/models/widget.rb",
            &format!("class Widget < ApplicationRecord\n  BASE = %i[draft live].freeze\n  enum :state, {mapping}\nend\n"),
        )
}

const FOLDS: &[&str] = &[
    "(BASE.dup << %i[archived]).flatten",
    "BASE + %i[archived]",
    "BASE.dup << :archived",
    "(BASE + %w[archived]).flatten.freeze",
];

const SCRIPT: &str = r#"
got = Widget.states
want = { "draft" => 0, "live" => 1, "archived" => 2 }
raise "states: #{got.inspect}" unless got == want
w = Widget.create!
raise "default: #{w.state}" unless w.state == "draft"
w.archived!
raise "bang: #{w.state}" unless Widget.find(w.id).archived?
puts "computed enum passed"
"#;

#[test]
fn a_mapping_computed_from_frozen_lists_folds() {
    for mapping in FOLDS {
        app(mapping).run_ruby(SCRIPT).assert_passes();
    }
}

#[test]
fn a_mapping_folded_from_a_mutated_constant_or_a_call_is_refused() {
    for mapping in ["BASE << :archived", "BASE.map(&:to_s)", "(BASE.dup << compute).flatten", "(BASE.dup << [[:a], 1]).flatten"] {
        let panic = std::panic::catch_unwind(|| app(mapping).ingest_warnings())
            .expect_err("this mapping must stay refused");
        let message = panic.downcast_ref::<String>().cloned().unwrap_or_default();
        assert!(message.contains("enum :state mapping"), "{mapping}: {message}");
    }
}

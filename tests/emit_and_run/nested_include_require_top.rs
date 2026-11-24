//! A model that includes a concern nested under its own name
//! (`Ledger::Stamping` for `include Stamping`) loads the concern's file
//! from the top of its own file, before `class Ledger < ApplicationRecord`.
//! The concern's file reopens the class with the same superclass, so it
//! can run first. Spinel places a file whose `require_relative` is a
//! top-level statement; one required from a class body only once every
//! other require of it is placed, which a cycle of requires never is.

use roundhouse::build_target::BuildTarget;

use super::emit_and_run;

const SCHEMA: &str = "ActiveRecord::Schema.define do\n  create_table :ledgers do |t|\n    t.string :name\n  end\nend\n";

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n",
        )
        .write(
            "app/controllers/application_controller.rb",
            "class ApplicationController < ActionController::Base\nend\n",
        )
        .write("db/schema.rb", SCHEMA)
        .write(
            "app/models/ledger.rb",
            "class Ledger < ApplicationRecord\n  include Stamping\n\n  def label\n    \"#{stamp}:#{name}\"\n  end\nend\n",
        )
        .write(
            "app/models/ledger/stamping.rb",
            "module Ledger::Stamping\n  extend ActiveSupport::Concern\n\n  def stamp\n    \"stamped\"\n  end\nend\n",
        )
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
}

fn ledger_file(tree: &std::path::Path) -> String {
    std::fs::read_to_string(tree.join("app/models/ledger.rb")).expect("app/models/ledger.rb is in the tree")
}

#[test]
fn the_spinel_tree_requires_a_nested_concern_above_its_class() {
    let (tree, errors) = app().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let text = ledger_file(&tree);
    let require = text.lines().position(|l| l == "require_relative \"ledger/stamping\"");
    let class = text.lines().position(|l| l.starts_with("class Ledger"));
    assert!(
        matches!((require, class), (Some(r), Some(c)) if r < c),
        "the concern's require is not a top-level statement above the class:\n{text}"
    );
    assert!(
        !text.lines().any(|l| l.starts_with(' ') && l.trim_start().starts_with("require_relative")),
        "a require is still in the class body:\n{text}"
    );
}

#[test]
fn a_nested_concern_required_above_its_class_runs_on_ruby() {
    app()
        .run_ruby(
            r#"
ledger = Ledger.new(name: "q1")
raise "label: #{ledger.label.inspect}" unless ledger.label == "stamped:q1"
raise "superclass: #{Ledger.superclass}" unless Ledger.superclass == ApplicationRecord
puts "nested concern above its class passed"
"#,
        )
        .assert_passes();
}

/// The concern reads a constant of another model whose own file reads this
/// class (`COPY = Audit::OWNER`, `OWNER = Ledger`): its file requires that
/// model's file, which would run before `class Ledger` if the concern's
/// require stood above it. It stays in the class body, where `ledger.rb`
/// loaded first (as its own entry) has the class open by then.
fn audit_app() -> emit_and_run::Overlay {
    app()
        .write(
            "app/models/ledger/stamping.rb",
            "module Ledger::Stamping\n  extend ActiveSupport::Concern\n  COPY = Audit::OWNER\n\n  def stamp\n    \"stamped\"\n  end\n\n  def copy = COPY\nend\n",
        )
        .write("app/models/audit.rb", "class Audit\n  OWNER = Ledger\nend\n")
}

#[test]
fn a_nested_concern_whose_file_requires_another_model_stays_in_the_class_body() {
    let (tree, errors) = audit_app().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let text = ledger_file(&tree);
    assert!(
        text.lines().any(|l| l == "  require_relative \"ledger/stamping\""),
        "the concern's require left the class body:\n{text}"
    );
}

#[test]
fn ledger_loaded_first_reads_the_concern_s_constant_on_ruby() {
    let (tree, errors) = audit_app().emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let out = std::process::Command::new("ruby")
        .current_dir(&*tree)
        .args(["-e", "require_relative 'app/models/ledger'; p Ledger::Stamping::COPY"])
        .output()
        .expect("run ruby");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.trim() == "Ledger",
        "ledger.rb as the entry did not load:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

//! A class nested in a library class's file (`Feeds::Strategy::Base` in
//! `feeds/strategy.rb`) whose body reads its outer module's constant
//! (`Feeds::LIMIT`) keeps that module's require when its own requires move
//! into the owner's file: `require_relative "../feeds"` from `feeds/`, not
//! `require_relative ""`.

use roundhouse::build_target::BuildTarget;

use super::emit_and_run;

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
        .write("lib/feeds.rb", "module Feeds\n  LIMIT = 3\nend\n")
        .write("lib/feeds/strategy.rb", "class Feeds::Strategy\n  def self.limit\n    Base::MAX\n  end\nend\n")
        .write("lib/feeds/strategy/base.rb", "class Feeds::Strategy::Base\n  MAX = Feeds::LIMIT * 2\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
}

fn rb_files(dir: &std::path::Path, out: &mut Vec<(std::path::PathBuf, String)>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rb_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rb") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                out.push((path, text));
            }
        }
    }
}

#[test]
fn a_nested_class_s_require_of_its_namespace_is_not_empty() {
    let (tree, errors) = app().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let mut files = Vec::new();
    rb_files(&tree, &mut files);
    let (path, owner) = files
        .iter()
        .find(|(_, text)| text.contains("class Base") && text.contains("Feeds::LIMIT"))
        .expect("Feeds::Strategy::Base is in the Spinel tree");
    assert!(!owner.lines().any(|l| l.trim() == "require_relative \"\""), "{} requires \"\":\n{owner}", path.display());
    assert!(owner.lines().any(|l| l.trim() == "require_relative \"../feeds\""), "{} does not require its namespace:\n{owner}", path.display());
}

#[test]
fn a_nested_class_reads_its_namespace_constant_on_ruby() {
    app()
        .run_ruby("raise \"limit: #{Feeds::Strategy.limit}\" unless Feeds::Strategy.limit == 6\nputs \"nested namespace require passed\"\n")
        .assert_passes();
}

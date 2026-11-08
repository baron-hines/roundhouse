//! A concern's `included` filters reach the controller the way Rails
//! registers them: through concerns whose only content is another
//! `include`, from `lib/`, and not through a plain Ruby module.

use std::collections::HashMap;
use std::path::PathBuf;

use roundhouse::ingest::ingest_app_from_tree;

#[path = "support/emit_and_run.rs"]
mod emit_and_run;
#[path = "support/concern_filter_reach_app.rs"]
mod concern_filter_reach_app;

#[test]
fn concern_filters_reach_controllers_as_rails_registers_them() {
    let run = concern_filter_reach_app::app().run_ruby(&format!(
        "{}{}",
        concern_filter_reach_app::LOAD_CONTROLLERS,
        concern_filter_reach_app::ASSERTIONS
    ));
    run.assert_passes();
    assert!(run.stdout.contains("concern filters reached"), "{}", run.stdout);
}

fn tree_with_included(included: &str, controller_includes: bool) -> HashMap<PathBuf, Vec<u8>> {
    let include = if controller_includes { "  include Gated\n" } else { "" };
    [
        (
            "app/controllers/concerns/gated.rb",
            format!(
                "module Gated\n  extend ActiveSupport::Concern\n\n  included do\n{included}\n  end\n\n  def check\n    head :unauthorized\n  end\nend\n"
            ),
        ),
        (
            "app/controllers/application_controller.rb",
            "class ApplicationController < ActionController::Base\nend\n".to_string(),
        ),
        (
            "app/controllers/widgets_controller.rb",
            format!("class WidgetsController < ApplicationController\n{include}\n  def index\n    render plain: \"ok\"\n  end\nend\n"),
        ),
        (
            "config/routes.rb",
            "Rails.application.routes.draw do\n  get \"/widgets\", to: \"widgets#index\"\nend\n".to_string(),
        ),
    ]
    .into_iter()
    .map(|(path, source)| (PathBuf::from(path), source.into_bytes()))
    .collect()
}

fn strict_refusal(included: &str) -> String {
    ingest_app_from_tree(tree_with_included(included, true))
        .err()
        .unwrap_or_else(|| panic!("expected a refusal for {included:?}"))
        .to_string()
}

#[test]
fn an_undecided_guard_around_a_filter_refuses_the_compile() {
    let error = strict_refusal("    before_action :check if ENV[\"LOCKED\"]");
    assert!(error.contains("`before_action` under a condition"), "{error}");
}

#[test]
fn an_unmodelled_callback_macro_refuses_the_compile() {
    let error = strict_refusal("    append_before_action :check");
    assert!(error.contains("macro not recognized: `append_before_action`"), "{error}");
}

#[test]
fn a_filter_registered_from_a_loop_refuses_the_compile() {
    let error = strict_refusal("    %i[check].each { |name| before_action name }");
    assert!(error.contains("`before_action` inside another statement"), "{error}");
}

#[test]
fn an_unread_filter_in_a_concern_no_controller_includes_is_not_a_refusal() {
    ingest_app_from_tree(tree_with_included("    before_action :check if ENV[\"LOCKED\"]", false))
        .expect("a concern no controller includes registers nothing");
}

/// A concern with no body of its own is a module the strict emitters now
/// see; it must not trip them.
#[test]
fn a_concern_that_only_includes_emits_on_the_strict_targets() {
    use roundhouse::project::BuildTarget;
    for target in [BuildTarget::Rust, BuildTarget::Typescript] {
        let (_, errors) = concern_filter_reach_app::app().emit(target);
        assert!(errors.is_empty(), "{target:?}: {errors:#?}");
    }
}

#[test]
fn a_filter_registered_from_an_included_hook_refuses_the_compile() {
    let mut tree = tree_with_included("", true);
    tree.insert(
        PathBuf::from("app/controllers/concerns/gated.rb"),
        b"module Gated\n  def self.included(base)\n    base.before_action :check if base.respond_to?(:before_action)\n  end\n\n  def check\n    head :unauthorized\n  end\nend\n".to_vec(),
    );
    let error = ingest_app_from_tree(tree).expect_err("a hook-registered filter is not modelled").to_string();
    assert!(error.contains("`before_action` in a `self.included` hook"), "{error}");
}

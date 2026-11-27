//! `render(template: name)` where `name` is a local the program fills in
//! (an error-page helper:
//! `template = admin? ? "errors/admin/generic_error" : "errors/generic_error"`
//! then `render(template: template, status:, layout: false, locals: {...})`).
//!
//! A view is one compiled method found by a name written in the source,
//! so a name known only at run time has no method to call. The render was
//! left as written and ended in the runtime's `render(body, ...)`:
//! `ArgumentError: wrong number of arguments (given 0, expected 1)`. It
//! now raises `ActionView::MissingTemplate` naming the template, as a
//! literal name of an unemitted view does.
//!
//! Asserted on the EMITTED TEXT: the raise is what the controller says.

use std::collections::HashMap;
use std::path::PathBuf;

use roundhouse::emit::ruby;
use roundhouse::ingest::ingest_app_from_tree;

fn controller_src(action_body: &str) -> String {
    let controller = format!(
        "class ThingsController < ApplicationController\n  def show\n    {action_body}\n  end\nend\n"
    );
    let mut app = ingest_app_from_tree(
        [
            (
                "db/schema.rb",
                "ActiveRecord::Schema.define(version: 1) do\n  create_table :things do |t|\n    t.string :name\n  end\nend\n".to_string(),
            ),
            ("app/models/thing.rb", "class Thing < ApplicationRecord\nend\n".to_string()),
            ("config/routes.rb", "Rails.application.routes.draw do\n  resources :things\nend\n".to_string()),
            ("app/controllers/things_controller.rb", controller),
        ]
        .into_iter()
        .map(|(p, c)| (PathBuf::from(p), c.into_bytes()))
        .collect::<HashMap<_, _>>(),
    )
    .expect("ingest");
    roundhouse::session::analyze_and_lower(&mut app);
    let files = ruby::emit_lowered_controllers_with_layout(&app);
    files
        .iter()
        .find(|f| f.path.ends_with("things_controller.rb"))
        .expect("things_controller.rb")
        .content
        .clone()
}

/// The raise names the value, and the render it replaces is gone.
#[test]
fn a_template_named_by_a_local_raises_missing_template() {
    let src = controller_src(
        "template = params[:admin] ? \"errors/admin/generic_error\" : \"errors/generic_error\"\n    render(template: template, status: 404, layout: false, locals: { message: \"x\" })",
    );
    assert!(
        src.contains("raise ActionView::MissingTemplate.new(template.to_s)"),
        "the render must raise MissingTemplate naming the template:\n{src}"
    );
    assert!(!src.contains("render(template:"), "the unresolvable render must not stay:\n{src}");
}

/// A literal name is still resolved at lowering.
#[test]
fn a_template_named_by_a_literal_is_not_this_raise() {
    let src = controller_src(r#"render(template: "things/show", status: 200)"#);
    assert!(!src.contains("MissingTemplate.new(\"things/show\".to_s)"), "{src}");
}

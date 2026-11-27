//! #2191: `it.permit(*shared_fields, :x)` on an element of
//! `params.require(:list)`.
//!
//! #1989 lowers an element's `permit(:a, :b)` to `Params.allowed_scalars`;
//! a splat argument made it decline, and the call stayed `permit` on the
//! element, a plain String-keyed Hash. Rails flattens the filters, so a
//! splat of a literal list, or of a controller method answering one (as
//! #858 reads it for the typed params chain), is that list written inline.
//! A splat of anything else stays as written.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::project::{target_files, BuildTarget};

const SCHEMA: &str = "ActiveRecord::Schema.define(version: 1) do\n  \
    create_table :widgets do |t|\n    t.string :name\n    t.string :kind\n    t.integer :size\n  end\nend\n";

/// The emitted `widgets_controller` and `application_controller`, with `helpers` as the former's private methods
/// and a `create` action that saves each of `widget_list_params`. `base` is
/// the body of the `ApplicationController` it inherits from.
fn controller_for(target: BuildTarget, helpers: &str, base: &str) -> String {
    controller_with(target, helpers, base, &[])
}

/// [`controller_for`] with `extra` source files (path, body) in the tree.
fn controller_with(target: BuildTarget, helpers: &str, base: &str, extra: &[(&str, &str)]) -> String {
    let controller = format!(
        "class WidgetsController < ApplicationController\n  def create\n    widget_list_params.each {{ |attrs| Widget.create!(attrs) }}\n    head :created\n  end\n\n  private\n\n{helpers}\nend\n"
    );
    let mut tree: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    tree.insert(PathBuf::from("db/schema.rb"), SCHEMA.as_bytes().to_vec());
    tree.insert(PathBuf::from("app/models/widget.rb"), b"class Widget < ApplicationRecord\nend\n".to_vec());
    tree.insert(
        PathBuf::from("config/routes.rb"),
        b"Rails.application.routes.draw do\n  resources :widgets, only: :create\nend\n".to_vec(),
    );
    tree.insert(
        PathBuf::from("app/controllers/application_controller.rb"),
        format!("class ApplicationController < ActionController::Base\n{base}\nend\n").into_bytes(),
    );
    tree.insert(PathBuf::from("app/controllers/widgets_controller.rb"), controller.into_bytes());
    for (path, body) in extra {
        tree.insert(PathBuf::from(path), body.as_bytes().to_vec());
    }
    let mut app = ingest_app_from_tree(tree).expect("ingest");
    roundhouse::session::analyze_and_lower(&mut app);
    let files = target_files(&app, Path::new("fixtures/tiny-blog"), target).expect("emit");
    files
        .into_iter()
        .filter(|(p, _)| p.ends_with("widgets_controller.rb") || p.ends_with("application_controller.rb"))
        .map(|(_, c)| c)
        .collect::<Vec<_>>()
        .join("\n")
}

const ALLOWED: &str = r#"Params.allowed_scalars(it, ["name", "kind", "size"])"#;
const LIST_PARAMS: &str = "  def widget_list_params\n    params.require(:widgets).map do\n      it.permit(*shared_fields, :size)\n    end\n  end\n";

#[test]
fn a_splat_of_a_helper_list_is_the_list() {
    for list in ["%i[name kind]", "%w[name kind]", "[:name, \"kind\"].freeze"] {
        let helpers = format!("  def shared_fields\n    {list}\n  end\n\n{LIST_PARAMS}");
        for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
            let src = controller_for(target, &helpers, "");
            assert!(src.contains(ALLOWED), "{target:?} {list}:\n{src}");
            assert!(!src.contains(".permit("), "{target:?} {list}:\n{src}");
        }
    }
}

/// A helper defined on the superclass, and a splat of a literal list.
#[test]
fn an_inherited_helper_and_a_literal_splat_are_read() {
    let base = "  private\n\n  def shared_fields\n    %i[name kind]\n  end\n";
    let src = controller_for(BuildTarget::Spinel, LIST_PARAMS, base);
    assert!(src.contains(ALLOWED), "{src}");
    let helpers = "  def widget_list_params\n    params.require(:widgets).map { it.permit(*%i[name kind], :size) }\n  end\n";
    let src = controller_for(BuildTarget::Spinel, helpers, "");
    assert!(src.contains(ALLOWED), "{src}");
}

/// A helper whose list is not a literal stays as written.
#[test]
fn a_splat_of_a_computed_list_stays() {
    let helpers = format!("  def shared_fields\n    [:name, :kind].reject {{ |f| f == :kind }}\n  end\n\n{LIST_PARAMS}");
    let src = controller_for(BuildTarget::Spinel, &helpers, "");
    assert!(!src.contains("allowed_scalars"), "{src}");
}

/// A list helper a descendant overrides: the ancestor's body runs on the descendant too, where Rails' `*helper`
/// answers the descendant's list, so the ancestor's list is not written into it.
#[test]
fn a_helper_a_descendant_overrides_is_not_inlined() {
    let base = format!("  private\n\n  def shared_fields\n    %i[name kind]\n  end\n\n{LIST_PARAMS}");
    let src = controller_for(BuildTarget::Spinel, "  def shared_fields\n    %i[name]\n  end\n", &base);
    assert!(!src.contains(ALLOWED), "{src}");
    assert!(!src.contains(r#"["name", "size"]"#), "{src}");
    // Overridden with the same list, or not at all: inlined as before.
    let src = controller_for(BuildTarget::Spinel, "  def shared_fields\n    %w[name kind]\n  end\n", &base);
    assert!(src.contains(ALLOWED), "{src}");
    let src = controller_for(BuildTarget::Spinel, "", &base);
    assert!(src.contains(ALLOWED), "{src}");
}

/// A concern a descendant includes overrides the helper as the descendant's own method does: the ancestor's list
/// is not inlined. A module the tree does not define cannot be read, so it may override the helper too.
#[test]
fn a_helper_an_included_module_may_override_is_not_inlined() {
    let base = format!("  private\n\n  def shared_fields\n    %i[name kind]\n  end\n\n{LIST_PARAMS}");
    let concern = "module NarrowFields\n  extend ActiveSupport::Concern\n\n  private\n\n  def shared_fields\n    %i[name]\n  end\nend\n";
    let src = controller_with(
        BuildTarget::Spinel,
        "  include NarrowFields\n",
        &base,
        &[("app/controllers/concerns/narrow_fields.rb", concern)],
    );
    assert!(!src.contains(ALLOWED), "{src}");
    let src = controller_with(BuildTarget::Spinel, "  include ExternalFields\n", &base, &[]);
    assert!(!src.contains(ALLOWED), "{src}");
}

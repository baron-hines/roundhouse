//! #2183: a Symbol-key write on an element of `params.require(:list)`.
//!
//! #1989 lowered the element's Symbol reads to the String keys the
//! request's Hash holds; a write beside them kept its Symbol key. Rails'
//! element is a Parameters, where `item[:kind] = v` and `item["kind"] = v`
//! write one key, so the write now stores the String key the reads read.
//! Natively the Symbol store raised on the String-keyed Hash; on the Ruby
//! family the later `allowed_scalars` read `"kind"` and dropped the value.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::project::{target_files, BuildTarget};

const SCHEMA: &str = "ActiveRecord::Schema.define(version: 1) do\n  \
    create_table :widgets do |t|\n    t.string :name\n    t.string :kind\n  end\nend\n";

/// The emitted `widgets_controller` with `body` as its `create` action.
fn controller_for(target: BuildTarget, body: &str) -> String {
    let controller = format!(
        "class WidgetsController < ApplicationController\n  def create\n{body}\n    head :created\n  end\nend\n"
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
        b"class ApplicationController < ActionController::Base\nend\n".to_vec(),
    );
    tree.insert(PathBuf::from("app/controllers/widgets_controller.rb"), controller.into_bytes());
    let mut app = ingest_app_from_tree(tree).expect("ingest");
    roundhouse::session::analyze_and_lower(&mut app);
    let files = target_files(&app, Path::new("fixtures/tiny-blog"), target).expect("emit");
    files
        .into_iter()
        .filter(|(p, _)| p.ends_with("widgets_controller.rb"))
        .map(|(_, c)| c)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_symbol_key_write_stores_the_string_key() {
    let body = "    items = params.require(:widgets).map do\n      it[:kind] = it[:kind].upcase if it[:kind]\n      it.permit(:name, :kind)\n    end\n    items.each { |attrs| Widget.create!(attrs) }";
    for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
        let src = controller_for(target, body);
        assert!(src.contains(r#"it["kind"] = it["kind"].upcase"#), "{target:?}:\n{src}");
        assert!(!src.contains("[:kind]"), "{target:?}:\n{src}");
    }
}

/// `||=` writes the same key, and a named block parameter is the element too.
#[test]
fn an_or_assign_on_a_named_element_stores_the_string_key() {
    let body = "    params.require(:widgets).each do |w|\n      w[:kind] ||= \"plain\"\n      Widget.create!(w.permit(:name, :kind))\n    end";
    let src = controller_for(BuildTarget::Spinel, body);
    assert!(src.contains(r#"w["kind"] ||= "plain""#), "{src}");
}

/// A nested block or lambda that rebinds the element's name, under any parameter kind, holds its own value:
/// the element lowering does not rewrite its Symbol keys to the element's String keys.
#[test]
fn a_rebound_element_name_is_left_alone() {
    for binding in ["->(w = { kind: \"old\" }) { w[:kind] = \"new\"; w[:kind] }", "->(*w) { w[:kind] }", "->(w: {}) { w[:kind] = \"new\" }", "->(&w) { w[:kind] }"] {
        let body = format!("    params.require(:widgets).each do |w|\n      relabel = {binding}\n      Widget.create!(w.permit(:name, :kind))\n    end");
        let src = controller_for(BuildTarget::Ruby, &body);
        assert!(!src.contains(r#"w["kind"]"#), "{binding}:\n{src}");
        assert!(src.contains(r#"Params.allowed_scalars(w, ["name", "kind"])"#), "{binding}:\n{src}");
    }
}

/// A block that reassigns the element's name holds another value there: its keys are not the element's.
#[test]
fn a_reassigned_element_name_is_left_alone() {
    let body = "    items = params.require(:widgets).map do |w|\n      w = { kind: \"old\" }\n      w[:kind] = \"new\"\n      w\n    end\n    items.each { |attrs| Widget.create!(attrs) }";
    let src = controller_for(BuildTarget::Ruby, body);
    assert!(src.contains("w[:kind] = \"new\""), "{src}");
    assert!(!src.contains(r#"w["kind"]"#), "{src}");
}

/// A nested block that reassigns its own parameter of the element's name does not stop the element's rewrite.
#[test]
fn a_nested_blocks_own_reassignment_keeps_the_rewrite() {
    let body = "    params.require(:widgets).each do |w|\n      w[:kind] = \"x\"\n      [1].each { |w| w = 2 }\n      Widget.create!(w.permit(:name, :kind))\n    end";
    let src = controller_for(BuildTarget::Ruby, body);
    assert!(src.contains(r#"w["kind"] = "x""#), "{src}");
}

/// A Symbol-key write as one target of a multiple assignment stores the String key too.
#[test]
fn a_multiple_assignment_index_target_stores_the_string_key() {
    let body = "    params.require(:widgets).each do |w|\n      w[:kind], other = \"new\", 0\n      Widget.create!(w.permit(:name, :kind))\n    end";
    let src = controller_for(BuildTarget::Ruby, body);
    assert!(src.contains(r#"w["kind"]"#), "{src}");
    assert!(!src.contains("w[:kind]"), "{src}");
}

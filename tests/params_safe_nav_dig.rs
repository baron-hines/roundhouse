//! #2188: `params[:spec]&.dig(:arch)` reads the String key.
//!
//! A nested params value is a Parameters in Rails, read by a Symbol as by
//! its String. `params[:spec].dig(:os)` was emitted with the String key;
//! the safe-navigation form, which ingest binds through a `__safe_nav`
//! temporary holding the same value, kept the Symbol and read nil from the
//! String-keyed Hash.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::project::{target_files, BuildTarget};

/// The emitted `gadgets_controller` with `body` as its `show` action.
fn controller_for(target: BuildTarget, body: &str) -> String {
    let controller = format!("class GadgetsController < ApplicationController\n  def show\n{body}\n  end\nend\n");
    let mut tree: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    tree.insert(
        PathBuf::from("db/schema.rb"),
        b"ActiveRecord::Schema.define(version: 1) do\n  create_table :gadgets do |t|\n    t.string :name\n  end\nend\n".to_vec(),
    );
    tree.insert(PathBuf::from("app/models/gadget.rb"), b"class Gadget < ApplicationRecord\nend\n".to_vec());
    tree.insert(
        PathBuf::from("config/routes.rb"),
        b"Rails.application.routes.draw do\n  resources :gadgets, only: :show\nend\n".to_vec(),
    );
    tree.insert(
        PathBuf::from("app/controllers/application_controller.rb"),
        b"class ApplicationController < ActionController::Base\nend\n".to_vec(),
    );
    tree.insert(PathBuf::from("app/controllers/gadgets_controller.rb"), controller.into_bytes());
    let mut app = ingest_app_from_tree(tree).expect("ingest");
    roundhouse::session::analyze_and_lower(&mut app);
    let files = target_files(&app, Path::new("fixtures/tiny-blog"), target).expect("emit");
    files
        .into_iter()
        .filter(|(p, _)| p.ends_with("gadgets_controller.rb"))
        .map(|(_, c)| c)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_safe_nav_dig_reads_the_string_keys() {
    let body = "    render json: { a: params[:spec].dig(:os), b: params[:spec]&.dig(:arch, :bits) }";
    for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
        let src = controller_for(target, body);
        assert!(src.contains(r#".dig("os")"#), "{target:?}:\n{src}");
        assert!(src.contains(r#".dig("arch", "bits")"#), "{target:?}:\n{src}");
        assert!(!src.contains(".dig(:"), "{target:?}:\n{src}");
    }
}

/// `&.[]` and `&.fetch` read the String key too.
#[test]
fn a_safe_nav_index_and_fetch_read_the_string_key() {
    let body = "    render json: { a: params[:spec]&.[](:os), b: params[:spec]&.fetch(:arch, nil) }";
    let src = controller_for(BuildTarget::Spinel, body);
    assert!(!src.contains("(:os)") && !src.contains("[:os]"), "{src}");
    assert!(src.contains(r#"fetch("arch", nil)"#), "{src}");
}

/// Only a value read out of the request's params keeps its String keys: a `permit` result is lowered to a
/// Symbol-keyed Hash, which its Symbol read finds.
#[test]
fn a_safe_nav_read_of_a_permit_result_keeps_its_symbol_key() {
    let body = "    render json: { a: params.permit(:name)&.dig(:name) }";
    let src = controller_for(BuildTarget::Spinel, body);
    assert!(!src.contains(r#"dig("name")"#), "{src}");
}

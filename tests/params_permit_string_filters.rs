//! #2186: `params.permit(%w[a b])` and `params.permit("a", "b")`.
//!
//! Rails' `Parameters#permit(*filters)` flattens its filters and takes a
//! String as it takes a Symbol, so both are `permit(:a, :b)`. The lowering
//! read only Symbol literals and left these calls as written, `permit` on
//! the request's plain Hash. They now lower like the Symbol form; a filter
//! list that is not a literal stays as written.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::project::{target_files, BuildTarget};

const SCHEMA: &str = "ActiveRecord::Schema.define(version: 1) do\n  \
    create_table :widgets do |t|\n    t.string :name\n    t.string :colour\n  end\nend\n";

/// The emitted `widgets_controller` with `body` as its `update` action.
fn controller_for(target: BuildTarget, body: &str) -> String {
    let controller = format!(
        "class WidgetsController < ApplicationController\n  def update\n{body}\n    head :ok\n  end\nend\n"
    );
    let mut tree: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    tree.insert(PathBuf::from("db/schema.rb"), SCHEMA.as_bytes().to_vec());
    tree.insert(PathBuf::from("app/models/widget.rb"), b"class Widget < ApplicationRecord\nend\n".to_vec());
    tree.insert(
        PathBuf::from("config/routes.rb"),
        b"Rails.application.routes.draw do\n  resources :widgets, only: :update\nend\n".to_vec(),
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

const PERMITTED: &str =
    r#"Params.permitted(Params.permitted({}, @params, "name", :name), @params, "colour", :colour)"#;

#[test]
fn string_and_array_filters_are_the_symbol_filters() {
    for filters in ["%w[name colour]", "\"name\", \"colour\"", "[:name, \"colour\"]", ":name, %w[colour]"] {
        let body = format!("    Widget.find(params[:id]).update!(params.permit({filters}))");
        for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
            let src = controller_for(target, &body);
            assert!(src.contains(PERMITTED), "{target:?} permit({filters}):\n{src}");
            assert!(!src.contains(".permit("), "{target:?} permit({filters}):\n{src}");
        }
    }
}

/// An element of `params.require(:list)` takes the same filters (#1989's lowering).
#[test]
fn an_element_takes_string_filters() {
    let body = "    params.require(:widgets).each { |w| Widget.create!(w.permit(%w[name colour])) }";
    let src = controller_for(BuildTarget::Spinel, body);
    assert!(src.contains(r#"Params.allowed_scalars(w, ["name", "colour"])"#), "{src}");
}

/// A list the source does not spell out stays as written.
#[test]
fn a_non_literal_filter_list_stays() {
    let body = "    fields = [params[:a]]\n    Widget.find(params[:id]).update!(params.permit(fields))";
    let src = controller_for(BuildTarget::Spinel, body);
    assert!(src.contains(".permit(fields)"), "{src}");
}

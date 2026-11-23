//! A concern method's keyword parameters are typed from its call sites
//! by name, as a model's or a controller helper's are.
//!
//! A call of a concern's method keys to the model that includes it,
//! which has no `def` of its own, so the call's keywords were never
//! placed on the module's keyword slots: the whole kwargs Hash landed on
//! the first slot, and `def label(name: nil, pinned: false)` called as
//! `label(name: "gear", pinned: true)` was declared
//! `(?name: Hash[Symbol, (bool | String)], ?pinned: bool)`. Since each
//! includer gets its own copy of a concern method that lowers
//! differently on it, spinel compiles the includer's copy against that
//! sidecar and the String argument meets an `sp_SymPolyHash *` slot.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::project::{target_files, BuildTarget};

const APPLICATION_RECORD: &str =
    "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n";
const APPLICATION_CONTROLLER: &str = "class ApplicationController < ActionController::Base\nend\n";
const SCHEMA: &str = r#"ActiveRecord::Schema.define do
  create_table "widgets", force: :cascade do |t|
    t.string "name"
  end
  create_table "gadgets", force: :cascade do |t|
    t.string "name"
  end
  create_table "tags", force: :cascade do |t|
    t.string "body"
    t.string "taggable_type"
    t.integer "taggable_id"
  end
end
"#;
const ROUTES: &str = "Rails.application.routes.draw do\n  root \"widgets#index\"\nend\n";

/// `label` is a plain concern method; `tag` calls the concern's own
/// `has_many`, so each includer gets a lowered copy of it.
const LABELLED: &str = r##"module Labelled
  extend ActiveSupport::Concern

  included do
    has_many :tags, as: :taggable
  end

  def label(name: nil, pinned: false)
    "#{name} #{pinned}"
  end

  def tag(name: nil, pinned: false)
    tags.create!(body: "#{name} #{pinned}")
  end
end
"##;

const WIDGETS_CONTROLLER: &str = r#"class WidgetsController < ApplicationController
  def index
    widget = Widget.find(params[:id])
    widget.tag(name: "gear", pinned: true)
    render plain: widget.label(name: "gear", pinned: true)
  end
end
"#;

fn spinel() -> Vec<(String, String)> {
    let mut tree: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    for (path, content) in [
        ("app/models/application_record.rb", APPLICATION_RECORD),
        ("app/controllers/application_controller.rb", APPLICATION_CONTROLLER),
        ("app/models/concerns/labelled.rb", LABELLED),
        ("app/models/widget.rb", "class Widget < ApplicationRecord\n  include Labelled\nend\n"),
        ("app/models/gadget.rb", "class Gadget < ApplicationRecord\n  include Labelled\nend\n"),
        ("app/models/tag.rb", "class Tag < ApplicationRecord\n  belongs_to :taggable, polymorphic: true\nend\n"),
        ("app/controllers/widgets_controller.rb", WIDGETS_CONTROLLER),
        ("db/schema.rb", SCHEMA),
        ("config/routes.rb", ROUTES),
    ] {
        tree.insert(PathBuf::from(path), content.as_bytes().to_vec());
    }
    let mut app = ingest_app_from_tree(tree).expect("ingest");
    roundhouse::session::analyze_and_lower(&mut app);
    target_files(&app, Path::new("."), BuildTarget::Spinel).expect("spinel files")
}

fn file<'a>(files: &'a [(String, String)], path: &str) -> &'a str {
    &files
        .iter()
        .find(|(p, _)| p == path)
        .unwrap_or_else(|| panic!("{path} not emitted"))
        .1
}

fn sig<'a>(rbs: &'a str, method: &str) -> &'a str {
    let head = format!("def {method}: ");
    rbs.lines()
        .map(str::trim)
        .find(|l| l.starts_with(&head))
        .unwrap_or_else(|| panic!("no sig for {method}:\n{rbs}"))
}

/// `name` is a String, however the sidecar spells the optional slot
/// (`?name: String` as a keyword, `?String name` once flattened).
fn names_a_string(sig: &str) -> bool {
    sig.contains("?name: String,") || sig.contains("?String name,")
}

#[test]
fn a_concern_keyword_is_typed_by_name() {
    let files = spinel();
    let rbs = file(&files, "app/models/labelled.rbs");
    let label = sig(rbs, "label");
    assert!(names_a_string(label), "{rbs}");
    assert!(!rbs.contains("Hash[Symbol"), "{rbs}");
}

#[test]
fn each_includer_copy_declares_the_keyword_by_name() {
    let files = spinel();
    for path in ["app/models/widget.rbs", "app/models/gadget.rbs"] {
        let rbs = file(&files, path);
        let tag = sig(rbs, "tag");
        assert!(names_a_string(tag), "{path}:\n{tag}");
        assert!(!tag.contains("Hash[Symbol"), "{path}:\n{tag}");
    }
}

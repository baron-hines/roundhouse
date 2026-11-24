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
    emit(&[
        ("app/models/concerns/labelled.rb", LABELLED),
        ("app/models/widget.rb", "class Widget < ApplicationRecord\n  include Labelled\nend\n"),
        ("app/models/gadget.rb", "class Gadget < ApplicationRecord\n  include Labelled\nend\n"),
        ("app/models/tag.rb", "class Tag < ApplicationRecord\n  belongs_to :taggable, polymorphic: true\nend\n"),
        ("app/controllers/widgets_controller.rb", WIDGETS_CONTROLLER),
    ])
}

/// The app of the base classes, the schema, the routes and `app_files`.
fn ingest(app_files: &[(&str, &str)]) -> roundhouse::App {
    let mut tree: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    for (path, content) in [
        ("app/models/application_record.rb", APPLICATION_RECORD),
        ("app/controllers/application_controller.rb", APPLICATION_CONTROLLER),
        ("db/schema.rb", SCHEMA),
        ("config/routes.rb", ROUTES),
    ]
    .iter()
    .chain(app_files)
    {
        tree.insert(PathBuf::from(path), content.as_bytes().to_vec());
    }
    ingest_app_from_tree(tree).expect("ingest")
}

/// The spinel tree for [`ingest`]'s app.
fn emit(app_files: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut app = ingest(app_files);
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

/// Ruby runs the includer's own `def` when it has one, even one whose
/// reopenings disagree on their parameters (which leaves it with no
/// shape to place keywords by). The module's keywords say nothing about
/// it: `label(name: "gear")` binds the whole Hash to `first`.
#[test]
fn an_includer_with_conflicting_defs_of_its_own_keeps_the_hash() {
    const KEYWORDED: &str = "module Keyworded\n  extend ActiveSupport::Concern\n\n  def label(name: nil)\n    name.to_s\n  end\nend\n";
    const WIDGET: &str = "class Widget < ApplicationRecord\n  include Keyworded\n\n  def label(first)\n    first.to_s\n  end\n\n  def label(first, second = nil)\n    first.to_s\n  end\nend\n";
    const CONTROLLER: &str = "class WidgetsController < ApplicationController\n  def index\n    render plain: Widget.find(params[:id]).label(name: \"gear\")\n  end\nend\n";
    let mut app = ingest(&[
        ("app/models/concerns/keyworded.rb", KEYWORDED),
        ("app/models/widget.rb", WIDGET),
        ("app/controllers/widgets_controller.rb", CONTROLLER),
    ]);
    // The sidecar types `first` `untyped` either way (the two `def`s
    // disagree), so read the row the call site left.
    let mut analyzer = roundhouse::analyze::Analyzer::new(&app);
    analyzer.analyze(&mut app);
    let row = app
        .inferred_method_params
        .get(&(roundhouse::ident::ClassId("Widget".into()), "label".into()))
        .expect("the label call site was collected");
    let first = format!("{:?}", row.first());
    assert!(first.contains("Hash"), "{row:?}");
}

/// Two modules define the method, and the one whose reopenings disagree
/// has no shape. That is a disagreement too, not a module to skip:
/// `Later`, included last, is the one Ruby reaches, and its `first`
/// takes the whole Hash.
#[test]
fn a_poisoned_module_shape_is_not_skipped_for_a_sibling() {
    const EARLIER: &str = "module Earlier\n  extend ActiveSupport::Concern\n\n  def label(name: nil)\n    name.to_s\n  end\nend\n";
    const LATER: &str = "module Later\n  extend ActiveSupport::Concern\n\n  def label(first)\n    first.to_s\n  end\n\n  def label(first, second = nil)\n    first.to_s\n  end\nend\n";
    const WIDGET: &str = "class Widget < ApplicationRecord\n  include Earlier\n  include Later\nend\n";
    const CONTROLLER: &str = "class WidgetsController < ApplicationController\n  def index\n    render plain: Widget.find(params[:id]).label(name: \"gear\")\n  end\nend\n";
    let files = emit(&[
        ("app/models/concerns/earlier.rb", EARLIER),
        ("app/models/concerns/later.rb", LATER),
        ("app/models/widget.rb", WIDGET),
        ("app/controllers/widgets_controller.rb", CONTROLLER),
    ]);
    let rbs = file(&files, "app/models/later.rbs");
    let label = sig(rbs, "label");
    assert!(label.contains("Hash[Symbol, String]"), "{rbs}");
    assert!(!label.contains("(String first"), "{rbs}");
}

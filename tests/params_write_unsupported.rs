//! A write into permitted params (`ps[:tags] = …`) lowers on the ruby
//! family and is reported on the strict targets. The permit lowers to a
//! synthesized record with a reader per key and no `[]=`; lobsters'
//! StoriesController#story_params assigns a Tag relation over its
//! `tags: []` Strings, which the ruby family's record carries by widening
//! the field (`Array[String] | Relation`). A strict target's field keeps
//! the request's type, so the write there stays an error rather than a
//! call to a method nothing defines.

use std::collections::HashMap;
use std::path::PathBuf;

use roundhouse::diagnostic::{DiagnosticKind, Severity};
use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::project::{BuildTarget, target_files};

fn writes_reported(helper_body: &str, target: BuildTarget) -> usize {
    let controller = format!(
        "class PostsController < ApplicationController\n  def update\n    @post = Post.find(params[:id])\n    @post.update(post_params)\n    head :ok\n  end\n\n  private\n\n  def post_params\n{helper_body}\n  end\nend\n"
    );
    let files: Vec<(&str, &str)> = vec![
        ("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table :posts do |t|\n    t.string :title\n  end\nend\n"),
        ("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n"),
        ("app/models/post.rb", "class Post < ApplicationRecord\nend\n"),
        ("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n"),
        ("app/controllers/posts_controller.rb", controller.as_str()),
        ("config/routes.rb", "Rails.application.routes.draw do\n  resources :posts, only: :update\nend\n"),
    ];
    let tree: HashMap<PathBuf, Vec<u8>> = files.iter().map(|(p, c)| (PathBuf::from(p), c.as_bytes().to_vec())).collect();
    let mut app = ingest_app_from_tree(tree).expect("ingest");
    let (_, lower_diags) = roundhouse::emit::diagnostics::scope(|| {
        roundhouse::session::analyze_and_lower_for(&mut app, target)
    });
    let (_, emit_diags) = roundhouse::emit::diagnostics::scope(|| {
        target_files(&app, roundhouse::fixtures::real_blog(), target)
    });
    lower_diags
        .iter()
        .chain(&emit_diags)
        .filter(|d| {
            d.severity == Severity::Error
                && matches!(&d.kind, DiagnosticKind::Unsupported { construct, .. } if construct.as_str() == "params write")
        })
        .count()
}

const WRITE: &str = "    ps = params.require(:post).permit(:title)\n    ps[:title] = ps[:title].to_s.strip\n    ps";

#[test]
fn a_write_into_permitted_params_is_reported_on_strict_targets() {
    for target in [BuildTarget::Rust, BuildTarget::Typescript] {
        assert_eq!(writes_reported(WRITE, target), 1, "{target:?}: the write is the one unsupported params construct");
    }
}

#[test]
fn a_write_into_permitted_params_lowers_on_the_ruby_family() {
    for target in [BuildTarget::Ruby, BuildTarget::Jruby, BuildTarget::Spinel] {
        assert_eq!(writes_reported(WRITE, target), 0, "{target:?}");
    }
}

#[test]
fn a_write_to_a_key_the_list_does_not_permit_is_reported_everywhere() {
    let write = "    ps = params.require(:post).permit(:title)\n    ps[:slug] = \"fixed\"\n    ps";
    for target in [BuildTarget::Ruby, BuildTarget::Spinel, BuildTarget::Rust] {
        assert_eq!(writes_reported(write, target), 1, "{target:?}");
    }
}

#[test]
fn reading_permitted_params_reports_nothing() {
    for target in [BuildTarget::Ruby, BuildTarget::Rust] {
        assert_eq!(writes_reported("    params.require(:post).permit(:title)", target), 0, "{target:?}");
    }
}

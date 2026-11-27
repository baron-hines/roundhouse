//! A gated permit key (`permit(:a, (:b if cond))`) lowers to one permit
//! and one `except`, whatever the number of gates, and a key also permitted
//! without a gate is never excepted, as Rails keeps it.

use std::collections::HashMap;
use std::path::PathBuf;

use roundhouse::ingest::ingest_app_from_tree;

fn helper_body(permit: &str) -> String {
    let controller = format!(
        "class PostsController < ApplicationController\n  def create\n    Post.create(post_params)\n    head :ok\n  end\n\n  private\n\n  def post_params\n    params.require(:post).permit({permit})\n  end\nend\n"
    );
    let files: Vec<(&str, &str)> = vec![
        ("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table :posts do |t|\n    t.string :title\n    t.string :body\n    t.string :note\n  end\nend\n"),
        ("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n"),
        ("app/models/post.rb", "class Post < ApplicationRecord\nend\n"),
        ("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n"),
        ("app/controllers/posts_controller.rb", controller.as_str()),
        ("config/routes.rb", "Rails.application.routes.draw do\n  resources :posts, only: :create\nend\n"),
    ];
    let tree: HashMap<PathBuf, Vec<u8>> = files.iter().map(|(p, c)| (PathBuf::from(p), c.as_bytes().to_vec())).collect();
    let app = ingest_app_from_tree(tree).expect("ingest");
    let controller = app.controllers.iter().find(|c| c.name.0.as_str() == "PostsController").expect("controller");
    let helper = controller.actions().find(|a| a.name.as_str() == "post_params").expect("helper");
    format!("{:?}", helper.body)
}

#[test]
fn gates_lower_to_one_permit_and_one_except() {
    let body = helper_body(":title, (:body if admin?), (:note if editor?), (:body if editor?)");
    assert_eq!(body.matches("\"permit\"").count(), 1, "{body}");
    assert_eq!(body.matches("\"except\"").count(), 1, "{body}");
}

#[test]
fn a_key_permitted_without_a_gate_is_kept() {
    let body = helper_body(":title, (:title if admin?)");
    assert!(!body.contains("\"except\""), "{body}");
}

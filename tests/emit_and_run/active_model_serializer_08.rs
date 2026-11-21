//! An active_model_serializers 0.8 serializer is expanded into plain
//! methods and renders as JSON without the gem, byte for byte as
//! active_model_serializers 0.8.4 does: a shared base's `embed :ids,
//! include: true`, inherited attributes, an `include_<name>?` override, a
//! `?` attribute losing its `?` in the key, ids with their records
//! sideloaded beside the root, embedded objects, `key:`, `root:`, a class
//! `root`, and `ActiveModel::ArraySerializer` with `each_serializer:`.

use super::emit_and_run;

const SERIALIZERS: &str = r#"
class ApplicationSerializer < ActiveModel::Serializer
  embed :ids, include: true
end

class CommentSerializer < ApplicationSerializer
  attributes :id, :commenter
  attribute :body, key: :text
end

class BasicArticleSerializer < ApplicationSerializer
  attributes :id, :title, :long?

  def long?
    object.body.to_s.length > 10
  end
end

class ArticleSerializer < BasicArticleSerializer
  attributes :summary, :draft
  has_many :comments, serializer: CommentSerializer
  has_one :first_comment, serializer: CommentSerializer, embed: :objects
  has_many :recent, serializer: CommentSerializer, embed: :objects, key: :latest
  has_one :last_comment, serializer: CommentSerializer, root: :comments

  def summary
    object.body.to_s[0, 5]
  end

  def draft
    object.title.start_with?("Draft")
  end

  def include_summary?
    !draft
  end

  def first_comment
    object.comments.first
  end

  def last_comment
    object.comments.last
  end

  def recent
    [object.comments.last]
  end
end

class ScopedArticleSerializer < ArticleSerializer
  root :post

  def include_title?
    scope == "admin"
  end
end

class PlainArticleSerializer < ActiveModel::Serializer
  root false
  attributes :id, :title
  has_many :comments, serializer: CommentSerializer
end
"#;

const CONTROLLER: &str = r#"class ArticleAmsController < ApplicationController
  def show
    render json: ArticleSerializer.new(Article.find(params[:id])).as_json
  end

  def draft
    render json: ArticleSerializer.new(Article.find(params[:id])).as_json
  end

  def admin
    render json: ScopedArticleSerializer.new(Article.find(params[:id]), scope: "admin").as_json
  end

  def guest
    render json: ScopedArticleSerializer.new(Article.find(params[:id]), scope: "guest").as_json
  end

  def plain
    render json: PlainArticleSerializer.new(Article.find(params[:id])).as_json
  end

  def index
    render json: ActiveModel::ArraySerializer.new(Article.order(:id), each_serializer: ArticleSerializer, root: "articles").as_json
  end

  def bare
    render json: ActiveModel::ArraySerializer.new(Article.order(:id), each_serializer: BasicArticleSerializer).as_json
  end

  def rootless
    ArticleSerializer.new(Article.find(params[:id]), root: false).as_json
    render plain: "no error"
  rescue StandardError => e
    render plain: e.message
  end
end
"#;

/// What active_model_serializers 0.8.4 renders for the same records
/// (articles 101 and 102, comments 201 and 202), from the real gem.
const EXPECTED: &[(&str, &str)] = &[
    ("show", r#"{"comments":[{"id":201,"commenter":"ann","text":"first"},{"id":202,"commenter":"bob","text":"second"}],"article":{"id":101,"title":"Hello","long":true,"summary":"A lon","draft":false,"comment_ids":[201,202],"first_comment":{"id":201,"commenter":"ann","text":"first"},"latest":[{"id":202,"commenter":"bob","text":"second"}],"last_comment_id":202}}"#),
    ("draft", r#"{"comments":[],"article":{"id":102,"title":"Draft one","long":false,"draft":true,"comment_ids":[],"first_comment":null,"latest":[null],"last_comment_id":null}}"#),
    ("admin", r#"{"comments":[{"id":201,"commenter":"ann","text":"first"},{"id":202,"commenter":"bob","text":"second"}],"post":{"id":101,"title":"Hello","long":true,"summary":"A lon","draft":false,"comment_ids":[201,202],"first_comment":{"id":201,"commenter":"ann","text":"first"},"latest":[{"id":202,"commenter":"bob","text":"second"}],"last_comment_id":202}}"#),
    ("guest", r#"{"comments":[{"id":201,"commenter":"ann","text":"first"},{"id":202,"commenter":"bob","text":"second"}],"post":{"id":101,"long":true,"summary":"A lon","draft":false,"comment_ids":[201,202],"first_comment":{"id":201,"commenter":"ann","text":"first"},"latest":[{"id":202,"commenter":"bob","text":"second"}],"last_comment_id":202}}"#),
    ("plain", r#"{"id":101,"title":"Hello","comments":[{"id":201,"commenter":"ann","text":"first"},{"id":202,"commenter":"bob","text":"second"}]}"#),
    ("index", r#"{"comments":[{"id":201,"commenter":"ann","text":"first"},{"id":202,"commenter":"bob","text":"second"}],"articles":[{"id":101,"title":"Hello","long":true,"summary":"A lon","draft":false,"comment_ids":[201,202],"first_comment":{"id":201,"commenter":"ann","text":"first"},"latest":[{"id":202,"commenter":"bob","text":"second"}],"last_comment_id":202},{"id":102,"title":"Draft one","long":false,"draft":true,"comment_ids":[],"first_comment":null,"latest":[null],"last_comment_id":null}]}"#),
    ("bare", r#"[{"id":101,"title":"Hello","long":true},{"id":102,"title":"Draft one","long":false}]"#),
    ("rootless", r#"Cannot serialize comments when ArticleSerializer does not have a root!"#),
];

fn overlay() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "  specs:\n", "  specs:\n    active_model_serializers (0.8.4)\n")
        .write("app/serializers/article_serializers.rb", SERIALIZERS)
        .write("app/controllers/article_ams_controller.rb", CONTROLLER)
        .edit(
            "config/routes.rb",
            "  resources :articles do",
            &format!(
                "  get \"article_ams\" => \"article_ams#index\"\n  get \"article_ams/bare\" => \"article_ams#bare\"\n{}  resources :articles do",
                ["show", "draft", "admin", "guest", "plain", "rootless"]
                    .map(|a| format!("  get \"article_ams/:id/{a}\" => \"article_ams#{a}\"\n"))
                    .concat()
            ),
        )
}

fn assertions() -> String {
    let mut out = String::from(
        r#"
require_relative "app/controllers/article_ams_controller"
a = Article.create!(title: "Hello", body: "A long body")
d = Article.create!(title: "Draft one", body: "short body")
c1 = Comment.create!(article_id: a.id, commenter: "ann", body: "first")
c2 = Comment.create!(article_id: a.id, commenter: "bob", body: "second")
fix = ->(s) { s.gsub("201", c1.id.to_s).gsub("202", c2.id.to_s).gsub("101", a.id.to_s).gsub("102", d.id.to_s) }
run = ->(action, id) { c = ArticleAmsController.new; c.params = id ? {"id" => id.to_s} : {}; c.process_action(action); c.body }
"#,
    );
    for (name, expected) in EXPECTED {
        let id = match *name {
            "draft" => "d.id",
            "index" | "bare" => "nil",
            _ => "a.id",
        };
        out.push_str(&format!(
            "got = run.(:{name}, {id})\nraise \"{name}: \" + got unless got == fix.({})\n",
            serde_json::to_string(expected).unwrap().replace('#', "\\#")
        ));
    }
    out.push_str("puts \"ams 0.8 passed\"\n");
    out
}

#[test]
fn an_active_model_serializer_08_renders_as_the_gem_does() {
    let run = overlay().run_ruby(&assertions());
    run.assert_passes();
    assert!(run.stdout.contains("ams 0.8 passed"));
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_active_model_serializer_08_renders_as_the_gem_does_on_spinel() {
    let run = overlay().run_spinel_with_db(&assertions());
    run.assert_passes();
    assert!(run.stdout.contains("ams 0.8 passed"));
}

/// A target that does not emit the expansion refuses each reference to a lowered serializer.
#[test]
fn an_active_model_serializer_08_is_refused_off_the_ruby_family() {
    let (_emitted, errors) = overlay().emit(roundhouse::project::BuildTarget::Python);
    assert!(errors.iter().any(|e| e.contains("ArticleSerializer") && e.contains("active_model_serializers 0.8")), "{errors:#?}");
}

const LOCK: &str = "GEM\n  remote: https://rubygems.org/\n  specs:\n    active_model_serializers (0.8.4)\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n  active_model_serializers\n";

fn tree(serializers: &str) -> std::collections::HashMap<std::path::PathBuf, Vec<u8>> {
    [
        ("Gemfile.lock", LOCK),
        ("app/serializers/serializers.rb", serializers),
        ("config/routes.rb", "Rails.application.routes.draw do\nend\n"),
    ]
    .into_iter()
    .map(|(p, s)| (std::path::PathBuf::from(p), s.as_bytes().to_vec()))
    .collect()
}

/// Each declaration outside the subset refuses its class (strict ingest fails, survey mode ledgers it) and every
/// class that inherits from it or serializes through it, which keep the gem's parent; the rest still lower.
#[test]
fn an_active_model_serializer_08_outside_the_subset_is_refused() {
    let base = "class BaseSerializer < ActiveModel::Serializer\n  attributes :id\nend\n\n";
    for (body, expected, refused) in [
        ("class ASerializer < BaseSerializer\n  has_many :comments\nend\n", "without `serializer:`", &["ASerializer"][..]),
        ("class ASerializer < BaseSerializer\n  attributes :display\nend\n", "an attribute named `display`", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  cached\nend\n", "`cached` is outside", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  has_one :author, serializer: BaseSerializer, polymorphic: true\nend\n", "`polymorphic:` is outside", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  def initialize(object, options = {})\n    super\n  end\nend\n", "`def initialize` would be replaced", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  def link\n    url_options\n  end\nend\n", "`url_options` is the gem's", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  def include!(name)\n    name\n  end\nend\n", "`def include!` would be replaced", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  private\n\n  def title\n    \"t\"\n  end\n\n  public\n\n  attributes :title\nend\n", "a private `title` before its declaration", &["ASerializer"]),
        ("module Api\n  class ASerializer < ::BaseSerializer\n  end\nend\n", "nested in `Api`", &["Api::ASerializer"]),
        ("class ASerializer < BaseSerializer\n  cached\nend\n\nclass BSerializer < ASerializer\nend\n", "inherits from ASerializer", &["ASerializer", "BSerializer"]),
        ("class ASerializer < BaseSerializer\n  cached\nend\n\nclass BSerializer < BaseSerializer\n  has_one :a, serializer: ASerializer\nend\n", "serializes through ASerializer", &["ASerializer", "BSerializer"]),
        ("class ASerializer < BaseSerializer\n  has_one :a, serializer: CommentSerializer\nend\n", "`serializer: CommentSerializer` is not a serializer of the app", &["ASerializer"]),
    ] {
        let tree = tree(&format!("{base}{body}"));
        assert!(roundhouse::ingest::ingest_app_from_tree(tree.clone()).is_err(), "{body}: strict ingest passed");
        roundhouse::ingest::survey::activate();
        let app = roundhouse::ingest::ingest_app_from_tree(tree).expect("survey ingest");
        let ledger: Vec<String> = roundhouse::ingest::survey::drain().iter().map(|e| e.to_string()).collect();
        assert!(ledger.iter().any(|e| e.contains(expected)), "{body}: {ledger:#?}");
        let lowered = |name: &str| {
            app.library_classes.iter().any(|lc| {
                lc.name.0.as_str() == name && lc.origin == Some(roundhouse::dialect::LibraryClassOrigin::ActiveModelSerializer08)
            })
        };
        for name in refused {
            assert!(!lowered(name), "{body}: {name} was lowered");
        }
        assert!(lowered("BaseSerializer"), "{body}: the base was not lowered");
    }
}

/// Under a 0.10 lockfile the 0.8 expansion stays out: a direct `ActiveModel::Serializer` child is 0.10's.
#[test]
fn an_active_model_serializer_08_needs_the_08_lockfile() {
    let mut tree = tree("class ASerializer < ActiveModel::Serializer\n  attributes :id\nend\n");
    tree.insert("Gemfile.lock".into(), LOCK.replace("0.8.4", "0.10.16").into_bytes());
    let app = roundhouse::ingest::ingest_app_from_tree(tree).expect("ingest");
    let a = app.library_classes.iter().find(|lc| lc.name.0.as_str() == "ASerializer").expect("ASerializer");
    assert!(matches!(a.origin, Some(roundhouse::dialect::LibraryClassOrigin::ActiveModelSerializer { .. })), "{:?}", a.origin);
}

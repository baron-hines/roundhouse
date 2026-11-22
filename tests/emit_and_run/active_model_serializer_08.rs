//! An active_model_serializers 0.8 serializer is expanded into plain
//! methods and renders as JSON without the gem, byte for byte as
//! active_model_serializers 0.8.4 does: a shared base's `embed :ids,
//! include: true`, inherited attributes, an `include_<name>?` override, a
//! `?` attribute losing its `?` in the key, ids with their records
//! sideloaded beside the root, embedded objects, `key:`, `root:`, a class
//! `root`, and `ActiveModel::ArraySerializer` with `each_serializer:`; an
//! association reading `object.comments.order(:id)` beside a serializer of
//! comments, and names (`h`, `node`) the expansion's own locals share; a
//! mixin whose `self.included` declares an attribute and whose gate and
//! private gate meet the gem's own, and one that only adds a method.

use super::emit_and_run;

const SERIALIZERS: &str = r##"
module FlairMixin
  def self.included(klass)
    klass.attributes :flair
  end

  def flair
    "f"
  end
end

module CommenterMixin
  def self.included(klass)
    klass.include FlairMixin
    klass.attributes :shout
  end

  def shout
    object.commenter.upcase + "!"
  end

  def include_shout?
    object.commenter != "bob"
  end

  private

  def include_body?
    false
  end
end

module StampMixin
  extend ActiveSupport::Concern

  included { attributes :stamp, :stamp_size }

  def stamp
    "st-" + object.title
  end

  def stamp_size
    stamp.length
  end
end

module NoteBase
  def summary_note
    "note"
  end
end

module NoteHelpers
  include NoteBase

  def helper_note
    "h"
  end
end

class ApplicationSerializer < ActiveModel::Serializer
  embed :ids, include: true

  def self.staff_attributes(*attrs)
    attributes(*attrs)
    attrs.each do |attr|
      method_name = "include_#{attr}?"
      define_method(method_name) do
        scope == "admin"
      end
    end
  end

  def self.compared(field)
    changes_name = :"#{field}_changes"
    attributes changes_name
    define_method(changes_name) do
      return if field == :none
      { now: object.public_send(field) }
    end
    define_method("include_#{changes_name}?") do
      !object.public_send(field).nil?
    end
  end
end

class CommentSerializer < ApplicationSerializer
  include CommenterMixin
  attr_accessor :note
  attributes :id, :commenter, :h, :note
  TAGS = %i[tag]
  TAGS.each { |v| public_send(:attr_accessor, v) }
  attributes :tag

  %i[body_length].each do |m|
    attributes m
    define_method m do
      object.body.to_s.public_send(:length)
    end
  end

  %i[commenter].each { |a| define_method("include_#{a}?") { object.commenter != "bob" } }
  attribute :body, key: :text

  def h
    commenter.upcase
  end

  def commenter
    object.commenter
  end
end

class BasicArticleSerializer < ApplicationSerializer
  attributes :id, :title, :long?, :raw_title, :body_size, :top_commenter, :pk_id
  delegate :title, to: :object, prefix: :raw
  delegate :body, to: :object, private: true
  delegate :commenter, to: :top, prefix: true, allow_nil: true

  def long?
    object.body.to_s.length > 10
  end

  def body_size
    body.to_s.length
  end

  def top
    object.comments.order(:id).first
  end

  alias include_top_commenter? long?
  compared :title

  private

  delegate :id, to: :object, prefix: :pk
end

class ArticleSerializer < BasicArticleSerializer
  include NoteHelpers
  attributes :summary, :draft, :summary_note
  has_many :comments, serializer: CommentSerializer
  has_one :first_comment, serializer: CommentSerializer, embed: :objects
  has_many :recent, serializer: CommentSerializer, embed: :objects, key: :latest
  has_one :last_comment, serializer: CommentSerializer, root: :comments
  has_many :older, serializer: CommentSerializer, root: false
  has_many :implicit_comments

  def implicit_comments
    object.comments.order(:id)
  end

  def older
    object.comments.order(:id).limit(1)
  end

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
    object.comments.order(:id).first
  end

  def last_comment
    object.comments.order(:id).last
  end

  def recent
    [object.comments.order(:id).last]
  end
end

class ScopedArticleSerializer < ArticleSerializer
  root :post
  staff_attributes :secret
  attributes :heading

  def heading
    "H"
  end

  def secret
    "s"
  end

  def initialize(object, opts = {})
    super(object, opts)
    @scoped = true
  end
  attributes :title_shown

  def title_shown
    include?(:title)
  end

  def include_title?
    scope == "admin"
  end

  alias_method :include_heading?, :include_title?
end

class PlainArticleSerializer < ActiveModel::Serializer
  root false
  include StampMixin
  attributes :id, :title, :greeting

  def self.flags
    %i[flag_a flag_b]
  end
  EXTRA = %i[greeting2]
  attributes(*flags, *EXTRA)
  flags.each { |f| define_method(f) { f == :flag_a } }

  def greeting2
    "g2"
  end

  def self.shouted(*list)
    [list].flatten.each do |attribute|
      attributes(attribute)
      class_eval %{def #{attribute}
        object.#{attribute}.to_s.upcase
      end}
    end
  end
  shouted :body

  def initialize(object, options = {})
    super
    @greeting = "hi " + object.title
  end

  def greeting
    @greeting
  end

  has_many :comments, serializer: CommentSerializer
  has_one :node, serializer: CommentSerializer, embed: :object, if: -> { false }
  has_one :first_c, embed: :objects

  def first_c
    object.comments.order(:id).first
  end

  def node
    object.comments.order(:id).last
  end
end
"##;

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

  def helper_show
    render_serialized(Article.find(params[:id]), ScopedArticleSerializer)
  end

  def helper_index
    render_serialized(Article.order(:id).to_a, BasicArticleSerializer, root: "articles")
  end

  private

  def serialize_data(obj, serializer, opts = nil)
    serializer_opts = { scope: "admin" }.merge!(opts || {})
    if obj.respond_to?(:to_ary)
      serializer_opts[:each_serializer] = serializer
      ActiveModel::ArraySerializer.new(obj.to_ary, serializer_opts).as_json
    else
      serializer.new(obj, serializer_opts).as_json
    end
  end

  def render_serialized(obj, serializer, opts = nil)
    render_json_dump(serialize_data(obj, serializer, opts), opts)
  end

  def render_json_dump(obj, opts = nil)
    render json: MultiJson.dump(obj)
  end
end
"#;

/// What active_model_serializers 0.8.4 renders for the same records
/// (articles 101 and 102, comments 201 and 202), from the real gem.
const EXPECTED: &[(&str, &str)] = &[
    ("show", r#"{"comments":[{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"},{"flair":"f","id":202,"h":"BOB","note":null,"tag":null,"body_length":6,"text":"second"}],"older":[{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"}],"implicit_comments":[{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"},{"flair":"f","id":202,"h":"BOB","note":null,"tag":null,"body_length":6,"text":"second"}],"article":{"id":101,"title":"Hello","long":true,"raw_title":"Hello","body_size":11,"top_commenter":"ann","pk_id":101,"title_changes":{"now":"Hello"},"summary":"A lon","draft":false,"summary_note":"note","comment_ids":[201,202],"first_comment":{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"},"latest":[{"flair":"f","id":202,"h":"BOB","note":null,"tag":null,"body_length":6,"text":"second"}],"last_comment_id":202,"older_ids":[201],"implicit_comment_ids":[201,202]}}"#),
    ("draft", r#"{"comments":[],"older":[],"implicit_comments":[],"article":{"id":102,"title":"Draft one","long":false,"raw_title":"Draft one","body_size":10,"pk_id":102,"title_changes":{"now":"Draft one"},"draft":true,"summary_note":"note","comment_ids":[],"first_comment":null,"latest":[null],"last_comment_id":null,"older_ids":[],"implicit_comment_ids":[]}}"#),
    ("admin", r#"{"comments":[{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"},{"flair":"f","id":202,"h":"BOB","note":null,"tag":null,"body_length":6,"text":"second"}],"older":[{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"}],"implicit_comments":[{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"},{"flair":"f","id":202,"h":"BOB","note":null,"tag":null,"body_length":6,"text":"second"}],"post":{"id":101,"title":"Hello","long":true,"raw_title":"Hello","body_size":11,"top_commenter":"ann","pk_id":101,"title_changes":{"now":"Hello"},"summary":"A lon","draft":false,"summary_note":"note","secret":"s","heading":"H","title_shown":true,"comment_ids":[201,202],"first_comment":{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"},"latest":[{"flair":"f","id":202,"h":"BOB","note":null,"tag":null,"body_length":6,"text":"second"}],"last_comment_id":202,"older_ids":[201],"implicit_comment_ids":[201,202]}}"#),
    ("guest", r#"{"comments":[{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"},{"flair":"f","id":202,"h":"BOB","note":null,"tag":null,"body_length":6,"text":"second"}],"older":[{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"}],"implicit_comments":[{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"},{"flair":"f","id":202,"h":"BOB","note":null,"tag":null,"body_length":6,"text":"second"}],"post":{"id":101,"long":true,"raw_title":"Hello","body_size":11,"top_commenter":"ann","pk_id":101,"title_changes":{"now":"Hello"},"summary":"A lon","draft":false,"summary_note":"note","title_shown":false,"comment_ids":[201,202],"first_comment":{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"},"latest":[{"flair":"f","id":202,"h":"BOB","note":null,"tag":null,"body_length":6,"text":"second"}],"last_comment_id":202,"older_ids":[201],"implicit_comment_ids":[201,202]}}"#),
    ("plain", r#"{"stamp":"st-Hello","stamp_size":8,"id":101,"title":"Hello","greeting":"hi Hello","flag_a":true,"flag_b":false,"greeting2":"g2","body":"A LONG BODY","comments":[{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"},{"flair":"f","id":202,"h":"BOB","note":null,"tag":null,"body_length":6,"text":"second"}],"node":{"flair":"f","id":202,"h":"BOB","note":null,"tag":null,"body_length":6,"text":"second"},"first_c":{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"}}"#),
    ("index", r#"{"comments":[{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"},{"flair":"f","id":202,"h":"BOB","note":null,"tag":null,"body_length":6,"text":"second"}],"older":[{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"}],"implicit_comments":[{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"},{"flair":"f","id":202,"h":"BOB","note":null,"tag":null,"body_length":6,"text":"second"}],"articles":[{"id":101,"title":"Hello","long":true,"raw_title":"Hello","body_size":11,"top_commenter":"ann","pk_id":101,"title_changes":{"now":"Hello"},"summary":"A lon","draft":false,"summary_note":"note","comment_ids":[201,202],"first_comment":{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"},"latest":[{"flair":"f","id":202,"h":"BOB","note":null,"tag":null,"body_length":6,"text":"second"}],"last_comment_id":202,"older_ids":[201],"implicit_comment_ids":[201,202]},{"id":102,"title":"Draft one","long":false,"raw_title":"Draft one","body_size":10,"pk_id":102,"title_changes":{"now":"Draft one"},"draft":true,"summary_note":"note","comment_ids":[],"first_comment":null,"latest":[null],"last_comment_id":null,"older_ids":[],"implicit_comment_ids":[]}]}"#),
    ("bare", r#"[{"id":101,"title":"Hello","long":true,"raw_title":"Hello","body_size":11,"top_commenter":"ann","pk_id":101,"title_changes":{"now":"Hello"}},{"id":102,"title":"Draft one","long":false,"raw_title":"Draft one","body_size":10,"pk_id":102,"title_changes":{"now":"Draft one"}}]"#),
    ("helper_show", r#"{"comments":[{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"},{"flair":"f","id":202,"h":"BOB","note":null,"tag":null,"body_length":6,"text":"second"}],"older":[{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"}],"implicit_comments":[{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"},{"flair":"f","id":202,"h":"BOB","note":null,"tag":null,"body_length":6,"text":"second"}],"post":{"id":101,"title":"Hello","long":true,"raw_title":"Hello","body_size":11,"top_commenter":"ann","pk_id":101,"title_changes":{"now":"Hello"},"summary":"A lon","draft":false,"summary_note":"note","secret":"s","heading":"H","title_shown":true,"comment_ids":[201,202],"first_comment":{"flair":"f","shout":"ANN!","id":201,"commenter":"ann","h":"ANN","note":null,"tag":null,"body_length":5,"text":"first"},"latest":[{"flair":"f","id":202,"h":"BOB","note":null,"tag":null,"body_length":6,"text":"second"}],"last_comment_id":202,"older_ids":[201],"implicit_comment_ids":[201,202]}}"#),
    ("helper_index", r#"{"articles":[{"id":101,"title":"Hello","long":true,"raw_title":"Hello","body_size":11,"top_commenter":"ann","pk_id":101,"title_changes":{"now":"Hello"}},{"id":102,"title":"Draft one","long":false,"raw_title":"Draft one","body_size":10,"pk_id":102,"title_changes":{"now":"Draft one"}}]}"#),
    ("rootless", r#"Cannot serialize comments when ArticleSerializer does not have a root!"#),
];

fn overlay() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "  specs:\n", "  specs:\n    active_model_serializers (0.8.4)\n    multi_json (1.21.1)\n    oj (3.17.6)\n")
        .write("config/initializers/100-oj.rb", "MultiJson.use(:oj)\n")
        .write("app/serializers/article_serializers.rb", SERIALIZERS)
        .write("app/controllers/article_ams_controller.rb", CONTROLLER)
        .edit(
            "config/routes.rb",
            "  resources :articles do",
            &format!(
                "  get \"article_ams\" => \"article_ams#index\"\n  get \"article_ams/bare\" => \"article_ams#bare\"\n  get \"article_ams/helper_index\" => \"article_ams#helper_index\"\n{}  resources :articles do",
                ["show", "draft", "admin", "guest", "plain", "rootless", "helper_show"]
                    .map(|a| format!("  get \"article_ams/:id/{a}\" => \"article_ams#{a}\"\n"))
                    .concat()
            ),
        )
}

/// An app's copy of the gem's `include!` with the rootless branch's `raise` dropped (0.9's behavior, as
/// discourse's `lib/freedom_patches/ams_include_without_root.rb` loads it).
const ROOTLESS_PATCH: &str = r#"module ActiveModel
  class Serializer
      def include!(name, options={})
        unique_values =
          if hash = options[:hash]
            if @options[:hash] == hash
              @options[:unique_values] ||= {}
            else
              {}
            end
          else
            hash = @options[:hash]
            @options[:unique_values] ||= {}
          end
  
        node = options[:node] ||= @node
        value = options[:value]
  
        if options[:include] == nil
          if @options.key?(:include)
            options[:include] = @options[:include].include?(name)
          elsif @options.include?(:exclude)
            options[:include] = !@options[:exclude].include?(name)
          end
        end
  
        association_class =
          if klass = _associations[name]
            klass
          elsif value.respond_to?(:to_ary)
            Associations::HasMany
          else
            Associations::HasOne
          end
  
        association = association_class.new(name, self, options)
  
        if association.embed_ids?
          node[association.key] = association.serialize_ids
  
          if association.embed_in_root? && hash.nil?
            # 0.9 skips what 0.8 raises for
          elsif association.embed_in_root? && association.embeddable?
            merge_association hash, association.root, association.serializables, unique_values
          end
        elsif association.embed_objects?
          node[association.key] = association.serialize
        end
      end
  
  end
end
"#;

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
            "index" | "bare" | "helper_index" => "nil",
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
fn an_active_model_serializer_08_under_the_app_s_rootless_patch_skips_the_sideload() {
    let script = assertions().replace(&serde_json::to_string("Cannot serialize comments when ArticleSerializer does not have a root!").unwrap(), "\"no error\"");
    assert!(script.contains("\"no error\""));
    let run = overlay()
        .write("lib/freedom_patches/ams_include_without_root.rb", ROOTLESS_PATCH)
        .write("config/initializers/002-freedom_patches.rb", "require Rails.root.join(\"lib/freedom_patches/ams_include_without_root\").to_s\n")
        .run_ruby(&script);
    run.assert_passes();
    assert!(run.stdout.contains("ams 0.8 passed"));
}

#[test]
fn an_active_model_serializer_08_under_another_reopening_of_the_gem_is_refused() {
    let tree = tree(&format!("{}\n\nclass ASerializer < ActiveModel::Serializer\n  attributes :id\nend\n", ROOTLESS_PATCH.replace("# 0.9 skips what 0.8 raises for", "nil")));
    roundhouse::ingest::survey::activate();
    let _ = roundhouse::ingest::ingest_app_from_tree(tree).expect("survey ingest");
    let ledger: Vec<String> = roundhouse::ingest::survey::drain().iter().map(|e| e.to_string()).collect();
    assert!(ledger.iter().any(|e| e.contains("the app reopens `ActiveModel::Serializer`")), "{ledger:#?}");
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
        ("class ASerializer < BaseSerializer\n  attributes :display\nend\n", "an attribute named `display`", &["ASerializer"][..]),
        ("class ASerializer < BaseSerializer\n  cached\nend\n", "`cached` is outside", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  has_one :author, serializer: BaseSerializer, polymorphic: true\nend\n", "`polymorphic:` is outside", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  has_one :author, serializer: BaseSerializer, if: check\nend\n", "`if:` is outside", &["ASerializer"]),
        ("module M\n  included { attributes :x }\n\n  def x\n    1\n  end\nend\n\nclass ASerializer < BaseSerializer\n  include M\nend\n", "does not `extend ActiveSupport::Concern`", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  def self.m(*list)\n    list.each { |a| class_eval %{def #{a}\n  1\nend\nputs 1} }\n  end\n  m :x\nend\n", "a `class_eval` string holding other than methods", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  def self.m(*list)\n    list.each { |a| class_eval %{def #{a}_#{object}\n  1\nend} }\n  end\n  m :x\nend\n", "a `class_eval` string interpolating other than the loop's name", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  def initialize(object, *rest)\n    super\n  end\nend\n", "hands a keyword or splat on through `super`", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  def link\n    url_options\n  end\nend\n", "`url_options` is the gem's", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  def include!(name)\n    name\n  end\nend\n", "`def include!` would be replaced", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  private\n\n  def title\n    \"t\"\n  end\n\n  public\n\n  attributes :title\nend\n", "a private `title` before its declaration", &["ASerializer"]),
        ("module Api\n  class ASerializer < ::BaseSerializer\n  end\nend\n", "nested in `Api`", &["Api::ASerializer"]),
        ("class ASerializer < BaseSerializer\n  cached\nend\n\nclass BSerializer < ASerializer\nend\n", "inherits from ASerializer", &["ASerializer", "BSerializer"]),
        ("class ASerializer < BaseSerializer\n  cached\nend\n\nclass BSerializer < BaseSerializer\n  has_one :a, serializer: ASerializer\nend\n", "serializes through ASerializer", &["ASerializer", "BSerializer"]),
        ("class ASerializer < BaseSerializer\n  has_one :a, serializer: CommentSerializer\nend\n", "`serializer: CommentSerializer` is not a serializer of the app", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  include Missing\nend\n", "`include Missing`: not a module of the app", &["ASerializer"]),
        ("module M\n  include Comparable\n\n  def x\n    1\n  end\nend\n\nclass ASerializer < BaseSerializer\n  include M\nend\n", "`include Comparable`: not a module of the app", &["ASerializer"]),
        ("module M\n  def self.included(klass)\n    klass.attributes :x\n    puts klass\n  end\nend\n\nclass ASerializer < BaseSerializer\n  include M\nend\n", "may only declare on `klass`", &["ASerializer"]),
        ("module M\n  def self.included(klass)\n    klass.attributes :x\n  end\nend\n\nclass Other\n  include M\nend\n\nclass ASerializer < BaseSerializer\n  include M\nend\n", "also runs for Other", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  private\n\n  %i[a].each { |n| define_method(n) { 1 } }\nend\n", "a loop defining methods after `private`", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  %i[a].each { |n| define_method(\"#{n}_#{object}\") { 1 } }\nend\n", "spelled from other than the loop's name", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  delegate :title, to: :object, prefix: PREFIX\nend\n", "a `prefix:` other than true or a Symbol", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  delegate :attributes, to: :object\nend\n", "`delegate` of `attributes`, which the expansion defines", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  alias include_id? id\nend\n", "`alias include_id? id` of a method the body does not define", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  def self.pair(f)\n    n = :\"#{f}_x\"\n    define_method(n) { n }\n  end\n  pair :a\nend\n", "a `define_method` body reading a loop local", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  def self.pair(f)\n    attributes f\n  end\n  pair :a, :b\nend\n", "a one-parameter macro called with other than one name", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  def masked(name)\n    \"m\"\n  end\n  alias read_attribute_for_serialization masked\nend\n", "`alias read_attribute_for_serialization` would be replaced", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  def self.add(f, **o)\n    attributes f\n  end\n  add :id\nend\n", "`add` is outside", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  def self.add(f)\n    f = :\"#{f}_x\"\n    attributes f\n  end\n  add :id\nend\n", "a declaring loop reassigning its name", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  def self.f\n    %i[id]\n  end\n\n  def self.f\n    names\n  end\n  attributes(*f)\nend\n", "`attributes` takes Symbols and a Hash of keys", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  names.each { |n| attributes n }\nend\n", "over other than a literal list of Symbols", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\nend\n\nclass Thing\n  def read_attribute_for_serialization(name) = \"masked\"\nend\n", "its own `read_attribute_for_serialization`", &["ASerializer", "BaseSerializer"]),
        ("class ASerializer < BaseSerializer\nend\n\nclass Thing\n  private def read_attribute_for_serialization(name)\n    \"masked\"\n  end\nend\n", "its own `read_attribute_for_serialization`", &["ASerializer", "BaseSerializer"]),
        ("class ASerializer < BaseSerializer\nend\n\nclass Caller\n  def self.go(x)\n    ASerializer.new(x, scope_name: :viewer)\n  end\nend\n", "`scope_name:`", &["ASerializer", "BaseSerializer"]),
        ("module M\n  def self.included(klass)\n    klass.attributes :x\n  end\nend\n\nmodule M\n  def y\n    1\n  end\nend\n\nclass ASerializer < BaseSerializer\n  include M\nend\n", "reopened in more than one body", &["ASerializer"]),
        ("module M\n  def self.append_features(base)\n    super\n  end\n\n  def x\n    1\n  end\nend\n\nclass ASerializer < BaseSerializer\n  include M\nend\n", "`self.append_features`", &["ASerializer"]),
        ("class CSerializer < BaseSerializer\nend\n\nclass ASerializer < BaseSerializer\n  CSerializer = ::BaseSerializer\n  has_one :c, serializer: CSerializer\nend\n", "`serializer: CSerializer` is not a serializer of the app", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  has_one :thing\nend\n\nclass Thing\n  def active_model_serializer\n    BaseSerializer\n  end\nend\n", "defines its own `active_model_serializer`", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  attributes :ams_attributes\nend\n", "an attribute named `ams_attributes`", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\n  embed :ids, EMBED\nend\n", "`embed` options other than a literal Hash", &["ASerializer"]),
        ("class ASerializer < BaseSerializer\nend\n\nclass Thing\n  def read_attribute_for_serialization(name)\n    name\n  end\nend\n", "its own `read_attribute_for_serialization`", &["ASerializer", "BaseSerializer"]),
        ("class ASerializer < BaseSerializer\n  def read_attribute_for_serialization(name)\n    send(name)\n  end\nend\n", "`def read_attribute_for_serialization` would be replaced", &["ASerializer"]),
        ("class CSerializer < ActiveModel::Serializer\n  def attributes\n    super.merge(extra: 1)\n  end\nend\n", "`super` in `def attributes` reaches the gem's class", &["CSerializer"]),
        ("class BSerializer < BaseSerializer\nend\n\nclass CSerializer < ActiveModel::Serializer\n  cached\nend\n\nclass ASerializer < BSerializer\nend\n\nclass BSerializer\n  has_one :c, serializer: CSerializer\nend\n", "a reopened serializer", &["BSerializer", "ASerializer"]),
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
            let class = app.library_classes.iter().find(|lc| lc.name.0.as_str() == *name).unwrap_or_else(|| panic!("{body}: {name} is gone"));
            assert!(class.parent.is_some(), "{body}: {name} lost its parent");
            assert!(!lowered(name), "{body}: {name} was lowered");
        }
        if !refused.contains(&"BaseSerializer") {
            assert!(lowered("BaseSerializer"), "{body}: the base was not lowered");
        }
    }
}

/// A record whose `read_attribute_for_serialization` only sends the name reads as its reader does: no refusal.
#[test]
fn an_active_model_serializer_08_beside_a_sending_serialization_reader_lowers() {
    for reader in [
        "  alias read_attribute_for_serialization send\n",
        "  def read_attribute_for_serialization(attribute)\n    public_send(attribute)\n  end\n",
        "  def read_attribute_for_serialization(name) = public_send(name)\n",
    ] {
        let tree = tree(&format!("class ASerializer < ActiveModel::Serializer\n  attributes :id\nend\n\nclass Thing\n{reader}end\n"));
        let app = roundhouse::ingest::ingest_app_from_tree(tree).expect("ingest");
        let a = app.library_classes.iter().find(|lc| lc.name.0.as_str() == "ASerializer").expect("ASerializer");
        assert_eq!(a.origin, Some(roundhouse::dialect::LibraryClassOrigin::ActiveModelSerializer08), "{reader}");
    }
}

/// A mixin outside the subset is ledgered at the serializer's `include`: a gap in the mixin's own file would claim
/// that file's findings.
#[test]
fn an_active_model_serializer_08_mixin_refusal_is_ledgered_at_the_include() {
    let mut tree = tree("class ASerializer < ActiveModel::Serializer\n  include Helpers\nend\n");
    tree.insert("app/helpers/helpers.rb".into(), b"module Helpers\n  extend Comparable\n\n  def x\n    1\n  end\nend\n".to_vec());
    roundhouse::ingest::survey::activate();
    roundhouse::ingest::ingest_app_from_tree(tree).expect("survey ingest");
    let ledger: Vec<String> = roundhouse::ingest::survey::drain().iter().map(|e| e.to_string()).collect();
    let entry = ledger.iter().find(|e| e.contains("`include Helpers`")).unwrap_or_else(|| panic!("{ledger:#?}"));
    assert!(entry.contains("app/serializers/serializers.rb") && !entry.contains("app/helpers"), "{entry}");
}

/// The gem's API an ancestor of the app's defines is inherited, not written again on the child; and a mixin included
/// after a declaration sits above the class, whose generated reader still answers.
#[test]
fn an_active_model_serializer_08_keeps_what_the_app_defines_above_it() {
    let tree = tree("module Fields\n  def title\n    \"mixin\"\n  end\nend\n\nclass BaseSerializer < ActiveModel::Serializer\n  def include?(name)\n    name != :email\n  end\nend\n\nclass ASerializer < BaseSerializer\n  attributes :email, :title\n  include Fields\nend\n");
    let app = roundhouse::ingest::ingest_app_from_tree(tree).expect("ingest");
    let a = app.library_classes.iter().find(|lc| lc.name.0.as_str() == "ASerializer").expect("ASerializer");
    let names: Vec<&str> = a.methods.iter().map(|m| m.name.as_str()).collect();
    assert!(!names.contains(&"include?"), "{names:?}");
    assert!(names.contains(&"title"), "{names:?}");
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

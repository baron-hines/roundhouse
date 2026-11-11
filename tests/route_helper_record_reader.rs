#[path = "support/emit_and_run.rs"]
mod emit_and_run;
#[path = "support/prerequisite.rs"]
mod prerequisite;

use roundhouse::project::BuildTarget;

const START: &str = r#"class PostStart
  include ActiveModel::Model
  attr_reader :post

  def save
    @post = Post.create!(title: "first")
    true
  end
end
"#;

const CONTROLLER: &str = r#"class PostsController < ApplicationController
  def create
    start = PostStart.new
    if start.save
      redirect_to post_path(start.post)
    end
  end

  def empty_reader_path
    post_path(new_start.post)
  end

  def new_start
    @reader_calls = @reader_calls.to_i + 1
    PostStart.new
  end

  def reader_calls
    @reader_calls.to_i
  end
end
"#;

fn example(slug: bool) -> emit_and_run::Overlay {
    let post = if slug {
        "class Post < ApplicationRecord\n  def to_param\n    title\n  end\nend\n"
    } else {
        "class Post < ApplicationRecord\nend\n"
    };
    emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table :posts do |t|\n    t.string :title, null: false\n  end\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\n  resources :posts, only: [:show, :create]\nend\n")
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n")
        .write("app/models/post.rb", post)
        .write("app/models/post_start.rb", START)
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("app/controllers/posts_controller.rb", CONTROLLER)
}

#[test]
fn a_record_reader_is_projected_to_the_helpers_segment_type() {
    for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
        for slug in [false, true] {
            let (tree, _) = example(slug).emit(target);
            let controller =
                std::fs::read_to_string(tree.join("app/controllers/posts_controller.rb")).unwrap();
            let method = if slug { "to_param" } else { "id" };
            assert!(
                controller.contains("route_segment_0 = start.post")
                    && controller.contains("route_segment_0.nil?")
                    && controller.contains(&format!("route_segment_0.{method}")),
                "{controller}"
            );
            let sig_root = if target == BuildTarget::Ruby { "sig/" } else { "" };
            let reader = std::fs::read_to_string(tree.join(format!("{sig_root}app/models/post_start.rbs"))).unwrap();
            assert!(reader.contains("attr_reader post: Post?"), "{reader}");
            let helpers = std::fs::read_to_string(tree.join(format!("{sig_root}app/route_helpers.rbs"))).unwrap();
            let ty = if slug { "String" } else { "Integer" };
            assert!(
                helpers.contains(&format!("def self.post_path: ({ty} id) -> String")),
                "{helpers}"
            );
        }
    }
}

#[test]
fn creating_through_a_form_object_redirects_to_its_record() {
    let oracle = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/rails_oracle.rb");
    let control = format!(
        r#"
require {oracle:?}
exit 77 unless RailsOracle.available?("activerecord", "actionpack", sqlite3: ">= 2.9")
RailsOracle.activate("activerecord", "actionpack", sqlite3: ">= 2.9")
require "active_record"
require "action_controller"
ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
ActiveRecord::Schema.verbose = false
ActiveRecord::Schema.define do
  create_table(:posts) {{ |t| t.string :title }}
end
class Post < ActiveRecord::Base; end
{START}
routes = ActionDispatch::Routing::RouteSet.new
routes.draw {{ resources :posts, only: [:show, :create] }}
start = PostStart.new
start.save
puts routes.url_helpers.post_path(start.post)
class Post
  def to_param; title; end
end
puts routes.url_helpers.post_path(start.post)
begin
  routes.url_helpers.post_path(PostStart.new.post)
  raise "nil reader unexpectedly generated a path"
rescue ActionController::UrlGenerationError
  puts "nil reader: ActionController::UrlGenerationError"
end
"#
    );
    let output = std::process::Command::new("ruby")
        .args(["-e", &control])
        .output()
        .expect("Rails control");
    if output.status.code() == Some(77) {
        prerequisite::missing("record reader control needs Rails and sqlite3");
        return;
    }
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let paths = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        paths,
        "/posts/1\n/posts/first\nnil reader: ActionController::UrlGenerationError\n"
    );
    for (slug, expected) in [false, true].into_iter().zip(paths.lines()) {
        example(slug)
            .run_ruby(&format!(
                r#"
require_relative "app/controllers/posts_controller"
controller = PostsController.new
controller.create
raise controller.location.inspect unless controller.location == {expected:?}
raise controller.status.inspect unless controller.status == 302
puts controller.location
begin
  controller.empty_reader_path
  raise "nil reader unexpectedly generated a path"
rescue ActionController::UrlGenerationError
  puts "nil reader: ActionController::UrlGenerationError"
end
raise controller.reader_calls.inspect unless controller.reader_calls == 1
"#
            ))
            .assert_passes();
    }
}

#[test]
fn conflicting_or_unknown_writes_do_not_claim_a_record_reader() {
    for extra in [
        "def reset; @post = 'token'; end",
        "def reset(value); @post = value; end",
        "def reset; @post = Post; end",
        "attr_writer :post",
        "def post; 'token'; end",
    ] {
        let start = START.replacen("end\n", &format!("end\n  {extra}\n"), 1);
        let (tree, _) = example(false)
            .write("app/models/post_start.rb", &start)
            .emit(BuildTarget::Spinel);
        let controller =
            std::fs::read_to_string(tree.join("app/controllers/posts_controller.rb")).unwrap();
        assert!(
            !controller.contains("route_segment_"),
            "{extra}:\n{controller}"
        );
    }
}

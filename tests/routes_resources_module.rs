#[path = "support/prerequisite.rs"]
mod prerequisite;
#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::lower::routes::flatten_routes;
use std::process::Command;

const ROUTES: &str = r#"  namespace :api do
    resources :posts, module: :admin, only: :show, constraints: { id: /[0-9]+/ } do
      member { get :preview }
      resources :comments, module: "thread", only: :index
    end
  end
  resources :questions, only: [] do
    resource :answer, module: :questions, only: :create
  end
  resources :downloads, module: "admin/tools", controller: :exports, only: :index
  resource :profile, module: :accounts, only: :show
  resources :widgets, only: :index
"#;

const RAILS: &[&str] = &[
    "GET /api/posts/:id/preview api/admin/posts#preview preview_api_post",
    "GET /api/posts/:post_id/comments api/admin/thread/comments#index api_post_comments",
    "GET /api/posts/:id api/admin/posts#show api_post",
    "POST /questions/:question_id/answer questions/answers#create question_answer",
    "GET /downloads admin/tools/exports#index downloads",
    "GET /profile accounts/profiles#show profile",
    "GET /widgets widgets#index widgets",
];

#[test]
fn resource_modules_qualify_controllers_and_preserve_paths_and_helpers() {
    let tree = [(
        "config/routes.rb".into(),
        format!("Rails.application.routes.draw do\n{ROUTES}end\n").into_bytes(),
    )]
    .into_iter()
    .collect();
    let app = ingest_app_from_tree(tree).expect("ingest");
    let mut got: Vec<String> = flatten_routes(&app)
        .into_iter()
        .map(|r| {
            let controller = r
                .controller
                .0
                .as_str()
                .trim_end_matches("Controller")
                .split("::")
                .map(roundhouse::naming::snake_case)
                .collect::<Vec<_>>()
                .join("/");
            format!(
                "{} {} {}#{} {}",
                format!("{:?}", r.method).to_uppercase(),
                r.path,
                controller,
                r.action.as_str(),
                r.as_name
            )
        })
        .collect();
    let mut want: Vec<String> = RAILS.iter().map(|s| s.to_string()).collect();
    got.sort();
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn dynamic_resource_modules_are_reported() {
    let source = b"Rails.application.routes.draw do\n  resources :posts, module: ENV.fetch(\"MODULE\")\nend\n";
    let error = roundhouse::ingest::routes::ingest_routes(source, "config/routes.rb")
        .expect_err("dynamic module");
    assert!(format!("{error:?}").contains("`module:` is not a literal string or symbol"));
}

#[test]
fn rails_agrees_with_resource_module_routes() {
    let available = Command::new("ruby")
        .args(["-e", r#"gem "actionpack", "8.1.4"; require "action_controller""#])
        .output()
        .is_ok_and(|output| output.status.success());
    if !available {
        prerequisite::missing("Ruby with actionpack 8.1.4 is not available");
        return;
    }
    let script = format!(
        r##"gem "actionpack", "8.1.4"
require "action_controller"
rs = ActionDispatch::Routing::RouteSet.new
rs.draw do
{ROUTES}end
rows = rs.routes.map {{ |r| [r.verb, r.path.spec.to_s.sub("(.:format)", ""), "#{{r.defaults[:controller]}}##{{r.defaults[:action]}}", r.name].join(" ") }}
raise rows.inspect unless rows.sort == {want:?}.sort
"##,
        want = RAILS.to_vec()
    );
    let output = Command::new("ruby")
        .args(["-e", &script])
        .output()
        .expect("ruby");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn the_ruby_target_dispatches_to_resource_module_controllers() {
    emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema[8.1].define(version: 1) do\nend\n")
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::API\nend\n")
        .write("app/controllers/api/admin/posts_controller.rb", "class Api::Admin::PostsController < ApplicationController\n  def show\n    render plain: \"post:#{params[:id]}\"\n  end\n  def preview\n    render plain: \"preview:#{params[:id]}\"\n  end\nend\n")
        .write("app/controllers/api/admin/thread/comments_controller.rb", "class Api::Admin::Thread::CommentsController < ApplicationController\n  def index\n    render plain: \"comments:#{params[:post_id]}\"\n  end\nend\n")
        .write("app/controllers/questions/answers_controller.rb", "class Questions::AnswersController < ApplicationController\n  def create\n    render plain: \"answer:#{params[:question_id]}\"\n  end\nend\n")
        .write("app/controllers/admin/tools/exports_controller.rb", "class Admin::Tools::ExportsController < ApplicationController\n  def index\n    render plain: \"exports\"\n  end\nend\n")
        .write("app/controllers/accounts/profiles_controller.rb", "class Accounts::ProfilesController < ApplicationController\n  def show\n    render plain: \"profile\"\n  end\nend\n")
        .write("app/controllers/widgets_controller.rb", "class WidgetsController < ApplicationController\n  def index\n    render plain: \"widgets\"\n  end\nend\n")
        .write("config/routes.rb", &format!("Rails.application.routes.draw do\n{ROUTES}end\n"))
        .run_ruby(r##"require "stringio"
[
  ["GET", "/api/posts/7", "post:7"],
  ["GET", "/api/posts/7/preview", "preview:7"],
  ["GET", "/api/posts/7/comments", "comments:7"],
  ["POST", "/questions/3/answer", "answer:3"],
  ["GET", "/downloads", "exports"],
  ["GET", "/profile", "profile"],
  ["GET", "/widgets", "widgets"]
].each do |verb, path, expected|
  status, _, body = Main.run_rack("REQUEST_METHOD" => verb, "PATH_INFO" => path, "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
  raise "#{verb} #{path}: #{status} #{body.inspect}" unless status == 200 && body.join == expected
end
status, _, _ = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/api/posts/abc", "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
raise "resource constraint: #{status}" unless status == 404
puts "PASS"
"##)
        .assert_passes();
}

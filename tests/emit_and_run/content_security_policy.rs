//! lobsters' Content-Security-Policy: the app's policy with a nonce generator (`script-src` only), and a stories
//! controller's `content_security_policy(only: :show)` whose `img-src` lambda reads `@story`. Every response carries the
//! header Rails' middleware writes, with the request's nonce on `script-src` and the same nonce in the layout's
//! `<script nonce=…>`; the lambda runs as the header is built, after the action set `@story`. `EXPECTED` is what Rails
//! 8.1.4 answers for the same app (`a_policy_and_its_nonce_match_rails`), each nonce written `N`.

#[path = "../support/prerequisite.rs"]
mod prerequisite;

use super::emit_and_run;

const POLICY: &str = r#"Rails.application.configure do
  config.content_security_policy do |policy|
    policy.default_src :none
    policy.img_src :self, :data
    policy.script_src :self
  end

  config.content_security_policy_nonce_generator = ->(request) { SecureRandom.base64(16) }
  config.content_security_policy_nonce_directives = %w[script-src]
end
"#;

const CONTROLLER: &str = r#"class StoriesController < ApplicationController
  content_security_policy(only: :show) do |policy|
    policy.img_src :self, :data, -> { @story ? "https:" : "" }
  end

  def show
    @story = params[:id] == "1" ? "story" : nil
  end

  def index
  end
end
"#;

const LAYOUT: &str = "<html><head><script type=\"speculationrules\" nonce=\"<%= content_security_policy_nonce %>\">{}</script></head><body><%= yield %></body></html>\n";

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("app/controllers/stories_controller.rb", CONTROLLER)
        .write("app/views/layouts/application.html.erb", LAYOUT)
        .write("app/views/stories/show.html.erb", "<p>show</p>\n")
        .write("app/views/stories/index.html.erb", "<p>index</p>\n")
        .write("config/application.rb", "require_relative \"boot\"\nrequire \"rails/all\"\n\nmodule Probe\n  class Application < Rails::Application\n    config.load_defaults 8.1\n  end\nend\n")
        .write("config/initializers/content_security_policy.rb", POLICY)
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/story.rb", "class Story < ApplicationRecord\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\n  resources :stories, only: [:index, :show]\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema[8.1].define(version: 2026_01_01_000000) do\n  create_table \"stories\", force: :cascade do |t|\n    t.string \"title\"\n  end\nend\n",
        )
}

const EXPECTED: &str = "default-src 'none'; img-src 'self' data: https:; script-src 'self' 'nonce-N'\n\
default-src 'none'; img-src 'self' data: ; script-src 'self' 'nonce-N'\n\
default-src 'none'; img-src 'self' data:; script-src 'self' 'nonce-N'\n\
nonces differ\n";

/// `get(path)` answers `[header, body]`; each nonce must be the body's too, and a new one per request.
const ASSERTIONS: &str = r##"
nonces = []
answer = +""
["/stories/1", "/stories/2", "/stories"].each do |path|
  header, body = get(path)
  nonce = header[/'nonce-([^']+)'/, 1] or raise "no nonce in #{header.inspect}"
  raise "the layout's nonce is not the header's: #{body}" unless body.include?(%(nonce="#{nonce}"))
  nonces << nonce
  answer << header.sub(nonce, "N") << "\n"
end
answer << (nonces.uniq.length == 3 ? "nonces differ\n" : "nonces repeat\n")
print answer
"##;

const RACK_GET: &str = r#"def get(path)
  status, headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => path, "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
  raise "GET #{path} answered #{status}" unless status == 200
  text = +""
  body.each { |part| text << part }
  [headers["content-security-policy"].to_s, text]
end
"#;

#[test]
fn a_policy_its_nonce_and_a_controller_s_lambda_source_go_out_on_each_response() {
    let run = app().run_ruby(&format!("{RACK_GET}{ASSERTIONS}"));
    run.assert_passes();
    assert!(run.stdout.ends_with(EXPECTED), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_policy_its_nonce_and_a_controller_s_lambda_source_go_out_on_each_response_on_spinel() {
    let run = app().run_spinel(&format!(
        r##"def get(path)
  id = path.split("/")[2]
  request = ActionDispatch::Request.new
  request.request_method = "GET"
  request.path = path
  request.query_string = ""
  ActionController::Current.request = request
  controller = StoriesController.new
  controller.request = request
  ActionController::Current.controller = controller
  controller.request_method = "GET"
  controller.params = id.nil? ? {{}} : {{ "id" => id }}
  controller.process_action(id.nil? ? :index : :show)
  [controller.headers["Content-Security-Policy"].to_s, controller.body]
end
{ASSERTIONS}"##
    ));
    run.assert_passes();
    assert!(run.stdout.ends_with(EXPECTED), "{}", run.stdout);
}

#[test]
fn a_policy_and_its_nonce_match_rails() {
    let root = std::env::temp_dir().join(format!("roundhouse_csp_oracle_{}", std::process::id()));
    for (path, content) in [
        ("app/views/layouts/application.html.erb", LAYOUT),
        ("app/views/stories/show.html.erb", "<p>show</p>\n"),
        ("app/views/stories/index.html.erb", "<p>index</p>\n"),
    ] {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).expect("create app directory");
        std::fs::write(file, content).expect("write app file");
    }
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/rails_oracle.rb");
    let script = format!(
        r#"require {helper:?}
unless RailsOracle.available?("railties", "actionpack", "actionview")
  warn "skipping CSP oracle: #{{RailsOracle.label}} needs railties, actionpack and actionview"
  exit 77
end
RailsOracle.activate("railties", "actionpack", "actionview")
require "rails"
require "action_controller/railtie"
class CspOracle < Rails::Application
  config.root = {root:?}
  config.load_defaults 8.1
  config.eager_load = false
  config.hosts.clear
  config.logger = Logger.new(File::NULL)
  config.secret_key_base = "csp-oracle"
  routes.append {{ resources :stories, only: [:index, :show] }}
end
{POLICY}class ApplicationController < ActionController::Base
end
{CONTROLLER}CspOracle.initialize!
def get(path)
  response = Rack::MockRequest.new(CspOracle).get(path)
  raise "GET #{{path}} answered #{{response.status}}" unless response.status == 200
  [response.headers["content-security-policy"].to_s, response.body]
end
{ASSERTIONS}"#,
        root = root.display().to_string(),
    );
    let output = std::process::Command::new("ruby").args(["-e", &script]).output().expect("Rails oracle");
    let _ = std::fs::remove_dir_all(&root);
    if output.status.code() == Some(77) {
        prerequisite::missing(String::from_utf8_lossy(&output.stderr).trim());
        return;
    }
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED);
}

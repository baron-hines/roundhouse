//! A `_url` helper given `protocol:` and no `host:` (`story_url(1, protocol: "https")`): Rails writes the request's
//! host with that protocol, a trailing `://` or not. `EXPECTED` is what Rails 8.1.4 answers on
//! `http://example.org/plain`.

use super::emit_and_run;

const CONTROLLER: &str = r#"class PlainsController < ApplicationController
  def show
    render plain: [article_url(1, protocol: "https"), plain_url(protocol: "ftp"), article_url(1, protocol: "https://"), article_url(1)].join(" ")
  end
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  get \"/plain\", to: \"plains#show\", as: :plain\n")
        .write("app/controllers/plains_controller.rb", CONTROLLER)
}

const EXPECTED: &str = "https://example.org/articles/1 ftp://example.org/plain https://example.org/articles/1 http://example.org/articles/1";

#[test]
fn a_protocol_alone_keeps_the_request_s_host() {
    app()
        .run_ruby(&format!(
            r##"status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/plain", "QUERY_STRING" => "", "HTTP_HOST" => "example.org", "rack.url_scheme" => "http", "SERVER_NAME" => "example.org", "SERVER_PORT" => "80", "rack.input" => StringIO.new(""))
raise "GET /plain answered #{{status}}" unless status == 200
text = +""
body.each {{ |part| text << part }}
raise "urls: #{{text.inspect}}" unless text == {EXPECTED:?}
puts "url helper protocol passed"
"##
        ))
        .assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_protocol_alone_keeps_the_request_s_host_on_spinel() {
    app()
        .run_spinel(&format!(
            r##"request = ActionDispatch::Request.new
request.request_method = "GET"
request.path = "/plain"
request.query_string = ""
request.host = "example.org"
ActionController::Current.request = request
controller = PlainsController.new
controller.request = request
ActionController::Current.controller = controller
controller.request_method = "GET"
controller.params = {{}}
controller.process_action(:show)
raise "urls: #{{controller.body}}" unless controller.body == {EXPECTED:?}
puts "url helper protocol passed"
"##
        ))
        .assert_passes();
}

#[test]
fn a_protocol_alone_matches_rails() {
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/rails_oracle.rb");
    let script = format!(
        r##"require {helper:?}
unless RailsOracle.available?("railties", "actionpack", "actionview")
  warn "skipping url protocol oracle: #{{RailsOracle.label}} needs railties, actionpack and actionview"
  exit 77
end
RailsOracle.activate("railties", "actionpack", "actionview")
require "rails"
require "action_controller/railtie"
class UrlProtocolOracle < Rails::Application
  config.load_defaults 8.1
  config.eager_load = false
  config.hosts.clear
  config.logger = Logger.new(File::NULL)
  config.secret_key_base = "url-protocol-oracle"
  routes.append {{ resources :articles, only: [:show]; get "/plain", to: "plains#show", as: :plain }}
end
UrlProtocolOracle.initialize!
class ApplicationController < ActionController::Base
end
{CONTROLLER}print Rack::MockRequest.new(UrlProtocolOracle).get("http://example.org/plain").body
"##
    );
    let output = std::process::Command::new("ruby").args(["-e", &script]).output().expect("Rails oracle");
    if output.status.code() == Some(77) {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr).trim());
        return;
    }
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED);
}

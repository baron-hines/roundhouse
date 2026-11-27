//! Rails reads a `_url` call's `port:` by whether the key is there, not by its value: an explicit `port: nil` drops
//! the class's port. A library class's host may carry its own port (`class.example:3000`), which a `port:` key
//! replaces; a controller's url options always carry the request's port, so there it never shows. An explicit
//! `only_path: false` keeps the class's host. `EXPECTED` is what Rails 8.1.4 answers (`a_url_option_s_presence_matches_rails`).

use super::emit_and_run;

const HOSTED: &str = r#"class HostedsController < ApplicationController
  def show
    render plain: [article_url(1, only_path: false), article_url(1, protocol: "https", only_path: false)].join(" ")
  end

  private

  def default_url_options
    { host: "class.example" }
  end
end
"#;

const PORTED: &str = r#"class PortedsController < ApplicationController
  def show
    render plain: [article_url(1), article_url(1, protocol: "https", port: 8443), article_url(1, port: nil)].join(" ")
  end

  private

  def default_url_options
    { host: "class.example", port: 8443 }
  end
end
"#;

const ROUTES: &str = r#"class PortedRoutes
  class << self
    include Rails.application.routes.url_helpers

    def default_url_options
      { host: "class.example:3000" }
    end

    def urls
      [article_url(1), article_url(1, protocol: "https", port: 8443), article_url(1, port: nil), article_url(1, protocol: "https", only_path: false)].join(" ")
    end
  end
end
"#;

const EXPECTED: [&str; 3] = [
    "http://class.example/articles/1 https://class.example/articles/1",
    "http://class.example:8443/articles/1 https://class.example:8443/articles/1 http://class.example/articles/1",
    "http://class.example:3000/articles/1 https://class.example:8443/articles/1 http://class.example/articles/1 https://class.example:3000/articles/1",
];

// Not on Spinel: a controller's own `default_url_options` Hash literal does not compile there yet (see url_helper_protocol.rs).
fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  get \"/hosted\", to: \"hosteds#show\"\n  get \"/ported\", to: \"porteds#show\"\n")
        .write("app/controllers/hosteds_controller.rb", HOSTED)
        .write("app/controllers/porteds_controller.rb", PORTED)
        .write("lib/ported_routes.rb", ROUTES)
}

#[test]
fn a_url_option_s_presence_decides_it() {
    app()
        .run_ruby(&format!(
            r##"def body_of(path)
  response = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => path, "QUERY_STRING" => "", "HTTP_HOST" => "example.org", "rack.url_scheme" => "http", "SERVER_NAME" => "example.org", "SERVER_PORT" => "80", "rack.input" => StringIO.new(""))
  raise "GET #{{path}} answered #{{response[0]}}" unless response[0] == 200
  text = +""
  response[2].each {{ |part| text << part }}
  text
end
expected = {EXPECTED:?}
got = [body_of("/hosted"), body_of("/ported"), PortedRoutes.urls]
raise "urls: #{{got.inspect}}" unless got == expected
puts "url option presence passed"
"##
        ))
        .assert_passes();
}

#[test]
fn a_url_option_s_presence_matches_rails() {
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/rails_oracle.rb");
    let script = format!(
        r##"require {helper:?}
unless RailsOracle.available?("railties", "actionpack", "actionview")
  warn "skipping url option oracle: #{{RailsOracle.label}} needs railties, actionpack and actionview"
  exit 77
end
RailsOracle.activate("railties", "actionpack", "actionview")
require "rails"
require "action_controller/railtie"
class UrlOptionOracle < Rails::Application
  config.load_defaults 8.1
  config.eager_load = false
  config.hosts.clear
  config.logger = Logger.new(File::NULL)
  config.secret_key_base = "url-option-oracle"
  routes.append {{ resources :articles, only: [:show]; get "/hosted", to: "hosteds#show"; get "/ported", to: "porteds#show" }}
end
UrlOptionOracle.initialize!
class ApplicationController < ActionController::Base
end
{HOSTED}{PORTED}{ROUTES}print [Rack::MockRequest.new(UrlOptionOracle).get("http://example.org/hosted").body, Rack::MockRequest.new(UrlOptionOracle).get("http://example.org/ported").body, PortedRoutes.urls].join("\n")
"##
    );
    let output = std::process::Command::new("ruby").args(["-e", &script]).output().expect("Rails oracle");
    if output.status.code() == Some(77) {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr).trim());
        return;
    }
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED.join("\n"));
}

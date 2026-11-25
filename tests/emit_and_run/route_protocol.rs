//! `root to: "home#index", protocol: (… ? "https://" : "http://")` (lobsters): the option picks only the protocol a
//! `root_url` builds when the call passes none. `root_path`, and a `root_url` naming its own `protocol:` (lobsters'
//! `Rails.application.root_url`), answer as Rails does; a `root_url` leaving the protocol to the route is reported.

use super::emit_and_run;

const ROUTES: &str = "Rails.application.routes.draw do\n  root to: \"home#index\",\n    protocol: (Rails.env.production? ? \"https://\" : \"http://\"),\n    as: \"root\"\nend\n";

fn app(action: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("app/controllers/home_controller.rb", &format!("class HomeController < ApplicationController\n  def index\n{action}  end\nend\n"))
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/visit.rb", "class Visit < ApplicationRecord\nend\n")
        .write("config/routes.rb", ROUTES)
        .write("config/application.rb", APPLICATION)
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema[8.1].define(version: 2026_01_01_000000) do\n  create_table \"visits\", force: :cascade do |t|\n    t.string \"room\"\n  end\nend\n",
        )
}

const OWN_PROTOCOL: &str = "    render plain: \"#{root_path}|#{root_url(host: \"example.org\", protocol: \"https\")}|#{Rails.application.root_url}|#{Rails.application.routes.url_helpers.root_url(host: \"example.org\", protocol: \"https\")}\"\n";

/// lobsters' spelling of its absolute root: the app's own `Rails.application` method, through `url_helpers`.
const APPLICATION: &str = r##"require_relative "boot"
require "rails/all"

module Probe
  class Application < Rails::Application
    config.load_defaults 8.1
  end
end

class << Rails.application
  def root_url
    Rails.application.routes.url_helpers.root_url(host: "example.org", protocol: "https")
  end
end
"##;

/// Rails 8.1.4 answers `/|https://example.org/|https://example.org/|https://example.org/` for the same action.
#[test]
fn a_root_route_protocol_leaves_its_path_and_an_explicit_protocol_alone() {
    app(OWN_PROTOCOL)
        .run_ruby(
            r##"status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/", "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
raise "GET / answered #{status}" unless status == 200
text = +""
body.each { |part| text << part }
raise "root urls: #{text.inspect}" unless text == "/|https://example.org/|https://example.org/|https://example.org/"
puts "route protocol passed"
"##,
        )
        .assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_root_route_protocol_leaves_its_path_and_an_explicit_protocol_alone_on_spinel() {
    app(OWN_PROTOCOL)
        .run_spinel(
            r##"controller = HomeController.new
controller.request_method = "GET"
controller.params = {}
controller.process_action(:index)
raise "root urls: #{controller.body}" unless controller.body == "/|https://example.org/|https://example.org/|https://example.org/"
puts "route protocol passed"
"##,
        )
        .assert_passes();
}

#[test]
fn a_root_url_that_leaves_the_protocol_to_the_route_is_an_error() {
    let (_tree, errors) = app("    redirect_to root_url\n").emit(roundhouse::project::BuildTarget::Ruby);
    assert!(
        errors.iter().any(|e| e.contains("`root_url` takes its protocol from the `root` route's `protocol:`")),
        "no route protocol error: {errors:#?}"
    );
}

//! `alias to_s name` inside a concern's `included do`, of the `attr_reader :name` beside it (mission_control-jobs'
//! `IdentifiedByName`): the includer answers `to_s` with the name, as Ruby's alias of that reader does, and the
//! module's own `alias to_param id` still copies `id`. (`id` is MC's `name.parameterize` spelled in core String methods:
//! a module's self call is untyped, so `parameterize` is not grounded there — a separate gap.)

use super::emit_and_run;

const CONCERN: &str = r#"module IdentifiedByName
  extend ActiveSupport::Concern

  included do
    attr_reader :name
    alias to_s name
  end

  def initialize(name:)
    @name = name.to_s
  end

  def id
    name.downcase.tr(" ", "-")
  end

  alias to_param id
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("lib/identified_by_name.rb", CONCERN)
        .write("lib/server.rb", "class Server\n  include IdentifiedByName\nend\n")
        .write(
            "app/controllers/application_controller.rb",
            "class ApplicationController < ActionController::Base\n  def index\n    server = Server.new(name: \"Solid Queue\")\n    render plain: \"#{server.to_s}|#{server.to_param}|#{server.name}\"\n  end\nend\n",
        )
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\n  root to: \"application#index\"\nend\n")
        .write("db/schema.rb", "ActiveRecord::Schema[8.1].define(version: 1) do\nend\n")
}

const EXPECTED: &str = "Solid Queue|solid-queue|Solid Queue";

#[test]
fn an_included_alias_of_a_reader_answers_the_name() {
    app()
        .run_ruby(&format!(
            r##"status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/", "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
raise "GET / answered #{{status}}" unless status == 200
text = +""
body.each {{ |part| text << part }}
raise "aliases: #{{text.inspect}}" unless text == {EXPECTED:?}
puts "included alias passed"
"##
        ))
        .assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_included_alias_of_a_reader_answers_the_name_on_spinel() {
    app()
        .run_spinel(&format!(
            r##"controller = ApplicationController.new
controller.request_method = "GET"
controller.params = {{}}
controller.process_action(:index)
raise "aliases: #{{controller.body}}" unless controller.body == {EXPECTED:?}
puts "included alias passed"
"##
        ))
        .assert_passes();
}

//! Calls lobsters makes as it seeds and serves, which checked clean and then raised once emitted: ActiveSupport's
//! `SecureRandom.base58` / `base36`, a bare `Rails.logger.debug`, and `starts_with?` on a value a gem hands the app.

use super::emit_and_run;

const CONTROLLER: &str = r##"class ProbesController < ApplicationController
  def index
    code = SecureRandom.base58(12)
    token = SecureRandom.base36
    sized = SecureRandom.base58(nil).length + SecureRandom.base36(nil).length
    Rails.logger.debug
    alphabet = code.chars.all? { |c| "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".include?(c) }
    render plain: "#{code.length} #{alphabet} #{token.length} #{token.match?(/\A[0-9a-z]+\z/)} #{sized}"
  end
end
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  get \"/probes\", to: \"probes#index\"\n")
        .write("app/controllers/probes_controller.rb", CONTROLLER)
}

const EXPECTED: &str = "12 true 16 true 32";

#[test]
fn securerandom_and_a_bare_logger_call_answer_as_rails_does() {
    let run = app().run_ruby(&format!(
        r##"status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/probes", "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
raise "GET /probes answered #{{status}}" unless status == 200
text = +""
body.each {{ |part| text << part }}
raise "probe: #{{text.inspect}}" unless text == {EXPECTED:?}
raise "starts_with?" unless JSON.parse('{{"path":"/u/alice"}}')["path"].starts_with?("/u/")
puts "boot calls passed"
"##
    ));
    run.assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn securerandom_and_a_bare_logger_call_answer_as_rails_does_on_spinel() {
    app()
        .run_spinel(&format!(
            r##"controller = ProbesController.new
controller.request_method = "GET"
controller.params = {{}}
controller.process_action(:index)
raise "probe: #{{controller.body}}" unless controller.body == {EXPECTED:?}
puts "boot calls passed"
"##
        ))
        .assert_passes();
}

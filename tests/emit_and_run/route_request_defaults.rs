//! Literal verb defaults retain Rails' types; captures win and query values lose.
use super::emit_and_run;

const ROUTES: &str = include_str!("../support/route_request_defaults/routes.rb");
const CONTROLLER: &str = include_str!("../support/route_request_defaults/controller.rb");
const CHECKS: &str = r#"require "stringio"
ActionController::Base.allow_forgery_protection = false
def answer(method, path, query, body, headers)
  env = { "REQUEST_METHOD" => method, "PATH_INFO" => path, "QUERY_STRING" => query, "CONTENT_LENGTH" => "0" }
  Main.dispatch_core(env, StringIO.new(body))
end
raise "literal" unless answer("GET", "/literal", "text=query&number=9&yes=false", "", {})[1] == '"fixed"|:staff|1|true|false|nil'
raise "optional default" unless answer("GET", "/optional", "number=9", "", {})[1] == 'nil|nil|1|nil|nil|nil'
raise "capture" unless answer("GET", "/optional/9", "", "", {})[1] == 'nil|nil|"9"|nil|nil|nil'
raise "required capture" unless answer("GET", "/number/9", "", "", {})[1] == 'nil|nil|"9"|nil|nil|nil'
raise "helper" unless RouteHelpers.number_path == "/number/1"
raise "match" unless answer("POST", "/matched", "", "", {})[1] == '"fixed"|nil|nil|nil|nil|nil'
raise "last pair" unless answer("GET", "/duplicate", "", "", {})[1] == '"last"|nil|nil|nil|nil|nil'
recognized = Rails.application.routes.recognize_path("/literal")
raise "recognize type" unless recognized[:symbol] == :staff && recognized[:number] == 1 && recognized.key?(:absent)
"#;
fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("config/routes.rb", "  root \"articles#index\"\n", ROUTES)
        .write("app/controllers/defaults_controller.rb", CONTROLLER)
}
#[test]
fn typed_defaults_reach_the_ruby_dispatcher_and_recognizer() {
    let expected = include_str!("../support/route_request_defaults/expected.json");
    let parity = format!(r##"
require "json"
JSON.parse({expected:?}).each do |path, expected|
  path, query = path.split("?", 2)
  status, body = answer("GET", path, query.to_s, "", {{}})
  raise "#{{path}}: #{{status}} #{{body}}" unless status == 200 && body == expected
end
"##);
    app().run_ruby(&format!("{CHECKS}\n{parity}")).assert_passes();
}
#[test]
fn integration_requests_and_assert_recognizes_keep_default_types() {
    app().write("test/controllers/defaults_test.rb", r#"require "test_helper"
class DefaultsTest < ActionDispatch::IntegrationTest
  test "typed defaults" do
    get "/literal?text=query&number=9"
    assert_equal '"fixed"|:staff|1|true|false|nil', response.body
    assert_recognizes({ controller: "defaults", action: "show", text: "fixed", symbol: :staff, number: 1, yes: true, no: false, absent: nil }, "/literal")
  end
end
"#).run_test("test/controllers/defaults_test.rb").assert_passes();
}
#[test]
#[ignore = "requires the Spinel toolchain"]
fn typed_defaults_reach_native_params() {
    let script = r#"matched = ActionDispatch::Router.match("GET", "/literal", [RouteTable.root] + RouteTable.table)
raise "match" if matched.nil?
params = { "text" => "query", "number" => "9" }
RouteTable.route_defaults(matched.defaults, params, matched.path_params)
controller = DefaultsController.new
controller.params = params
controller.show
raise "types" unless controller.response.body == '"fixed"|:staff|1|true|false|nil'
puts "kind=fixed"
"#;
    app().run_spinel_with_rbs(script).assert_passes();
}

#[test]
fn rails_oracle_confirms_values_classes_helpers_and_precedence() {
    let output = std::process::Command::new("ruby")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/route_request_defaults/oracle.rb"))
        .output().expect("Rails oracle");
    if output.status.code() == Some(77) {
        super::prerequisite::missing(String::from_utf8_lossy(&output.stderr).trim());
        return;
    }
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

//! The app's own `config.middleware.use(Klass)` in config/application.rb,
//! of a class the app itself defines: the middleware sits where Rails puts
//! it, inside RequestId and before the routes.

use roundhouse::build_target::BuildTarget;

use super::emit_and_run;

const PROBE_USE: &str = r#"class ProbeUse
  def initialize(app)
    @app = app
  end

  def call(env)
    return [418, { "content-type" => "text/plain" }, ["use"]] if env["PATH_INFO"] == "/use"

    @app.call(env)
  end
end
"#;

const AUTOLOAD: &str = "    config.autoload_lib(ignore: %w[assets tasks])\n";

/// The blog with `ProbeUse` in lib/ and `lines` in its Application class body.
pub(crate) fn with_application_lines(overlay: emit_and_run::Overlay, lines: &str) -> emit_and_run::Overlay {
    overlay.write("lib/probe_use.rb", PROBE_USE).edit("config/application.rb", AUTOLOAD, &format!("{AUTOLOAD}{lines}"))
}

const ASSERTIONS: &str = r#"
def hit(path)
  env = { "REQUEST_METHOD" => "GET", "PATH_INFO" => path, "QUERY_STRING" => "", "REMOTE_ADDR" => "127.0.0.1",
          "HTTP_HOST" => "localhost", "rack.input" => StringIO.new("") }
  Main.run_rack(env)
end

status, headers, body = hit("/use")
raise "the middleware's own answer: #{status} #{body.inspect}" unless status == 418 && body == ["use"]
# Rails' RequestId sits outside the app's middleware: its own answer carries the id too.
raise "X-Request-Id #{headers.inspect}" if headers["x-request-id"].to_s.empty?
status, _headers, _body = hit("/articles")
raise "past the middleware: #{status}" unless status == 200
puts "app middleware passed"
"#;

#[test]
fn an_app_middleware_answers_as_in_rails() {
    let run = with_application_lines(emit_and_run::real_blog(), "    config.middleware.use ProbeUse\n").run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("app middleware passed"), "{}", run.stdout);
}

#[test]
fn the_spinel_tree_stacks_the_app_middleware() {
    let (tree, errors) =
        with_application_lines(emit_and_run::real_blog(), "    config.middleware.use ProbeUse\n").emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let stack = std::fs::read_to_string(tree.join("runtime/gem_middleware.rb")).expect("gem_middleware.rb");
    assert!(stack.contains("stack = ProbeUse.new(App.new)"), "{stack}");
}

/// The repro's app: no model, no view, one controller, and a
/// config/application.rb that only the middleware reader reads.
fn bare_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "config/application.rb",
            "require \"rails\"\nrequire \"action_controller/railtie\"\nrequire_relative \"../lib/probe_use\"\n\nmodule Probe\n  class Application < Rails::Application\n    config.load_defaults 8.1\n    config.middleware.use ProbeUse\n  end\nend\n",
        )
        .write("config/routes.rb", "Rails.application.routes.draw do\n  get \"/pages\", to: \"pages#index\"\nend\n")
        .write("lib/probe_use.rb", PROBE_USE)
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write(
            "app/controllers/pages_controller.rb",
            "class PagesController < ApplicationController\n  def index\n    render plain: \"pages\"\n  end\nend\n",
        )
}

#[test]
fn reading_the_middleware_registers_no_source_after_the_snapshot() {
    for (target, stacked) in [(BuildTarget::Ruby, "ProbeUse.new(app)"), (BuildTarget::Spinel, "stack = ProbeUse.new(App.new)")] {
        let (tree, _errors) = bare_app().emit(target);
        let stack = std::fs::read_to_string(tree.join("runtime/gem_middleware.rb")).expect("gem_middleware.rb");
        assert!(stack.contains(stacked), "{target:?}: {stack}");
    }
}

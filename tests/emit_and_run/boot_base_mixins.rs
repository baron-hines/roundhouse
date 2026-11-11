//! Devise's engine initializer runs `Devise.include_helpers(Devise::Controllers)`, whose body includes the
//! controller helpers as ActionController::Base loads (`ActiveSupport.on_load(:action_controller) { include … }`):
//! every controller of the app answers them, as each root controller's own `include` makes it.

use super::emit_and_run;

const KIT: &str = r##"module Kit
  def self.include_helpers(scope)
    ActiveSupport.on_load(:action_controller) do
      include scope::Helpers if defined?(scope::Helpers)
    end
  end

  module Helpers
    def kit_greeting
      "hi from kit"
    end
  end
end
"##;

const CONTROLLER: &str = r#"class GreetingsController < ApplicationController
  def show
    render plain: kit_greeting
  end
end
"#;

const SCRIPT: &str = r##"env = { "REQUEST_METHOD" => "GET", "PATH_INFO" => "/greeting", "QUERY_STRING" => "", "REMOTE_ADDR" => "127.0.0.1",
        "HTTP_HOST" => "localhost", "rack.input" => StringIO.new("") }
status, _, body = Main.run_rack(env)
text = body.respond_to?(:join) ? body.join : body.to_s
raise "#{status} #{text.inspect}" unless status == 200 && text == "hi from kit"
puts "boot base mixins passed"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("lib/kit.rb", KIT)
        .write("config/initializers/kit.rb", "Kit.include_helpers(Kit)\n")
        .write("app/controllers/greetings_controller.rb", CONTROLLER)
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  get \"greeting\", to: \"greetings#show\"\n")
}

#[test]
fn a_boot_call_that_includes_into_the_controller_base_reaches_every_controller() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("boot base mixins passed"), "{}", run.stdout);
}

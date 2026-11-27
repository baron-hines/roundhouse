//! Devise's controllers: `append_before_action`, AbstractController's alias of `before_action`,
//! runs as one does, and a class body's `alias :new :old` copies the method as `alias_method` does.

use super::emit_and_run;

const CONTROLLER: &str = r##"class GreetingsController < ApplicationController
  append_before_action :set_name
  before_action :set_tail

  def show
    render plain: "#{@name} #{greeting} #{@tail}"
  end

  private

  def set_name
    @name = "devise"
  end

  def set_tail
    @tail = "done"
  end

  def salute
    "hello"
  end
  alias :greeting :salute
end
"##;

const SCRIPT: &str = r##"env = { "REQUEST_METHOD" => "GET", "PATH_INFO" => "/greeting", "QUERY_STRING" => "", "REMOTE_ADDR" => "127.0.0.1",
        "HTTP_HOST" => "localhost", "rack.input" => StringIO.new("") }
status, _, body = Main.run_rack(env)
text = body.respond_to?(:join) ? body.join : body.to_s
raise "#{status} #{text.inspect}" unless status == 200 && text == "devise hello done"
puts "append callbacks passed"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/controllers/greetings_controller.rb", CONTROLLER)
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  get \"greeting\", to: \"greetings#show\"\n")
}

#[test]
fn append_before_action_and_a_keyword_alias_run_in_a_controller() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("append callbacks passed"), "{}", run.stdout);
}

//! A nested params value's `permit("a", "b")` and `permit(%w[a b])` keep
//! only the permitted scalars, as Rails' `permit` does: a Hash or an
//! Array under a scalar filter's name is dropped.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/strings\", to: \"filters#strings\"\n  get \"/list\", to: \"filters#list\"\n",
        )
        .write(
            "app/controllers/filters_controller.rb",
            r#"class FiltersController < ApplicationController
  def strings
    q = params.fetch(:q, {}).permit("name", "settings", "tags")
    render plain: [q.keys.sort, q[:name]].inspect
  end

  def list
    q = params.fetch(:q, {}).permit(%w[name settings tags])
    render plain: [q.keys.sort, q[:name]].inspect
  end
end
"#,
        )
}

const ASSERTIONS: &str = r##"
require "stringio"
def get(path, query)
  env = { "REQUEST_METHOD" => "GET", "PATH_INFO" => path, "QUERY_STRING" => query, "HTTP_ACCEPT" => "text/html" }
  Main.dispatch_core(env, StringIO.new(""))
end
def body_of(response)
  body = response[1]
  body.is_a?(Array) ? body.join : body.to_s
end
query = "q%5Bname%5D=w&q%5Bsettings%5D%5Ba%5D=1&q%5Btags%5D%5B%5D=x&q%5Btags%5D%5B%5D=y"
["/strings", "/list"].each do |path|
  response = get(path, query)
  raise "#{path} status #{response[0]}" unless response[0] == 200
  raise "#{path} kept #{body_of(response)}" unless body_of(response) == '[["name"], "w"]'
end
puts "params string filter scalars passed"
"##;

#[test]
fn string_filters_on_a_nested_value_keep_only_scalars() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("params string filter scalars passed"), "{}", run.stdout);
}

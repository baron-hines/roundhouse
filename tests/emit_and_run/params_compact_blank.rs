//! maybe's `TransactionsController#search_params`:
//! `params.fetch(:q, {}).permit(…).to_h.compact_blank` drops the blank
//! filters a search form submits, as ActiveSupport's `compact_blank` does.
//! The Hash `to_h` answers is Rails' indifferent one, read by either key.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/search\", to: \"searches#show\"\n",
        )
        .write(
            "app/controllers/searches_controller.rb",
            r#"class SearchesController < ApplicationController
  def show
    @q = search_params
    render plain: [@q[:search], @q[:amount], @q.key?(:start_date), @q.size].inspect
  end

  private

  def search_params
    params.fetch(:q, {}).permit(:start_date, :search, :amount, accounts: []).to_h.compact_blank
  end
end
"#,
        )
}

const ASSERTIONS: &str = r#"
require "stringio"
def get(path, query)
  env = { "REQUEST_METHOD" => "GET", "PATH_INFO" => path, "QUERY_STRING" => query, "HTTP_ACCEPT" => "text/html" }
  Main.dispatch_core(env, StringIO.new(""))
end
def body_of(response)
  body = response[1]
  body.is_a?(Array) ? body.join : body.to_s
end
filtered = get("/search", "q%5Bsearch%5D=coffee&q%5Bamount%5D=&q%5Bstart_date%5D=")
raise "status #{filtered[0]}" unless filtered[0] == 200
raise "compacted #{body_of(filtered)}" unless body_of(filtered) == '["coffee", nil, false, 1]'
empty = get("/search", "")
raise "empty #{empty[0]} #{body_of(empty)}" unless empty[0] == 200 && body_of(empty) == "[nil, nil, false, 0]"
puts "params compact_blank passed"
"#;

#[test]
fn permitted_params_compact_their_blank_values() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("params compact_blank passed"), "{}", run.stdout);
}

/// The Spinel binary has no Rack dispatcher: the controller takes the
/// request's String-keyed params, as its dispatch hands them over.
const SPINEL_ASSERTIONS: &str = r#"
def search(params)
  controller = SearchesController.new
  controller.request_method = "GET"
  controller.params = params
  controller.process_action(:show)
  controller.body
end
filtered = search({"q" => {"search" => "coffee", "amount" => "", "start_date" => ""}})
raise "compacted #{filtered}" unless filtered == '["coffee", nil, false, 1]'
empty = search({})
raise "empty #{empty}" unless empty == "[nil, nil, false, 0]"
puts "params compact_blank passed"
"#;

#[test]
#[ignore = "requires the Spinel toolchain"]
fn permitted_params_compact_their_blank_values_on_spinel() {
    let run = app().run_spinel(SPINEL_ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("params compact_blank passed"), "{}", run.stdout);
}

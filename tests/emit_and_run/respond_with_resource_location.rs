//! `respond_with` on a POST with no `location:`, json alone. The Responder's `api_location` is `url_for(resource)`,
//! which native resolves only from an instance variable's name. A resource from any other expression still answers
//! its errors with 422, and raises on success instead of answering with a wrong Location.

use super::emit_and_run;

const CONTROLLER: &str = r#"class ThingsController < ApplicationController
  skip_forgery_protection
  respond_to :json

  def meth
    respond_with saved
  end

  def meth_err
    respond_with unsaved
  end

  private

  def saved
    Article.create!(title: "m", body: "a body long enough to pass")
  end

  def unsaved
    article = Article.new(title: "", body: "a body long enough to pass")
    article.valid?
    article
  end
end
"#;

const ROUTES: &str = r#"  post "t/meth", to: "things#meth"
  post "t/meth_err", to: "things#meth_err"
"#;

#[test]
fn a_post_without_location_on_a_computed_resource_answers_errors_and_raises_on_success() {
    let run = emit_and_run::real_blog()
        .write("app/controllers/things_controller.rb", CONTROLLER)
        .edit("config/routes.rb", "  root \"articles#index\"\n", &format!("  root \"articles#index\"\n{ROUTES}"))
        .run_ruby(
            r##"
def hit(path)
  env = { "REQUEST_METHOD" => "POST", "PATH_INFO" => path, "QUERY_STRING" => "", "REMOTE_ADDR" => "127.0.0.1",
          "HTTP_HOST" => "localhost", "rack.input" => StringIO.new(""), "HTTP_ACCEPT" => "application/json" }
  status, headers, body = Main.run_rack(env)
  [status, (headers["location"] || headers["Location"]).to_s, body.join]
end
got = hit("/t/meth_err.json")
raise "meth_err: got #{got.inspect}" unless got[0] == 422 && got[1].empty? && got[2].include?("{\"errors\":{\"title\":")
got = begin
  hit("/t/meth.json")
rescue RuntimeError => e
  [500, "", e.message]
end
raise "meth: got #{got.inspect}" unless got[0] == 500 && got[2].include?("needs location:")
puts "respond_with location passed"
"##,
        );
    run.assert_passes();
    assert!(run.stdout.contains("respond_with location passed"), "{}", run.stdout);
}

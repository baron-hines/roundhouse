//! A redirect keeps the content type a filter assigned before it: Rails'
//! `redirect_to` sets none, and only a response whose type is still unset
//! defaults to text/html (`assign_default_content_type_and_charset!`).
//! The test below passes unchanged in the generated blog on Rails 8.1
//! (`bin/rails test`); the Rack script asks the served app the same.

const RACK: &str = r##"
[["/forward/typed", 301, "application/json; charset=utf-8"], ["/forward/plain", 302, "text/html; charset=utf-8"]].each do |path, code, type|
  status, headers, _body = Main.run_rack(
    "REQUEST_METHOD" => "GET",
    "PATH_INFO" => path,
    "QUERY_STRING" => "",
    "HTTP_HOST" => "localhost",
    "rack.input" => StringIO.new("")
  )
  raise "#{path} status #{status}" unless status == code
  raise "#{path} location #{headers.inspect}" unless headers["location"] == "http://localhost/articles"
  raise "#{path} content type #{headers.inspect}" unless headers["content-type"] == type
end
puts "redirect types passed"
"##;

use super::emit_and_run;

const FILE: &str = "test/controllers/forwards_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/forward/typed\", to: \"forwards#typed\"\n  get \"/forward/plain\", to: \"forwards#plain\"\n",
        )
        .write(
            "app/controllers/forwards_controller.rb",
            r#"class ForwardsController < ApplicationController
  before_action :answer_in_json, only: :typed

  def typed
    redirect_to articles_path, status: :moved_permanently
  end

  def plain
    redirect_to articles_path
  end

  private

  def answer_in_json
    self.content_type = "application/json"
  end
end
"#,
        )
        .write(
            FILE,
            r#"require "test_helper"

class ForwardsControllerTest < ActionDispatch::IntegrationTest
  test "a redirect keeps the type a filter set" do
    get "/forward/typed"
    assert_response :moved_permanently
    assert_redirected_to articles_url
    assert_equal "application/json", response.media_type
  end

  test "a redirect with no type set is html" do
    get "/forward/plain"
    assert_response :found
    assert_equal "text/html", response.media_type
  end
end
"#,
        )
}

#[test]
fn a_redirect_keeps_the_assigned_content_type() {
    app().run_test(FILE).assert_passes();
}

#[test]
fn a_served_redirect_keeps_the_assigned_content_type() {
    let run = app().run_ruby(RACK);
    run.assert_passes();
    assert!(run.stdout.contains("redirect types passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires spin and spinel on PATH plus SQLite development libraries"]
fn a_redirect_keeps_the_assigned_content_type_on_spinel() {
    let (emitted, errors) = app().emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    let file = "test/forwards_controller_test.rb";
    assert!(emitted.join(file).exists(), "{file} was not emitted in {}", emitted.display());
    let output = std::process::Command::new("spin").args(["test", file]).current_dir(&*emitted).output().expect("spawn spin");
    assert!(
        output.status.success(),
        "spin test failed in {}\n=== stdout ===\n{}\n=== stderr ===\n{}",
        emitted.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

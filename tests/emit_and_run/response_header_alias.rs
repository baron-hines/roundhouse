//! A test's `response.header` is `response.headers`: Rails'
//! `ActionDispatch::Response` aliases the two (`alias_method :header,
//! :headers`), and a read through either is case-insensitive. The test
//! below passes unchanged in the generated blog on Rails 8.1
//! (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/shelf_marks_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "app/controllers/articles_controller.rb",
            "  def show\n",
            "  def show\n    response.headers[\"X-Shelf-Mark\"] = \"aisle-4\"\n",
        )
        .write(
            FILE,
            r#"require "test_helper"

class ShelfMarksControllerTest < ActionDispatch::IntegrationTest
  test "header reads what headers holds" do
    get article_url(articles(:two))
    assert_response :success
    assert_equal "aisle-4", response.header["X-Shelf-Mark"]
    assert_equal "aisle-4", response.header["x-shelf-mark"]
    assert_equal "aisle-4", @response.header["X-Shelf-Mark"]
    assert_equal response.headers["Content-Type"], response.header["Content-Type"]
    assert_nil response.header["X-Never-Set"]
  end
end
"#,
        )
}

#[test]
fn response_header_reads_the_response_headers() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "requires spin and spinel on PATH plus SQLite development libraries"]
fn response_header_reads_the_response_headers_on_spinel() {
    let (emitted, errors) = app().emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    let file = "test/shelf_marks_controller_test.rb";
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

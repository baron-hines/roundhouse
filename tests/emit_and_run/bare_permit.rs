use super::bare_permit_contract as contract;

const TEST: &str = r#"require "test_helper"

class FindsControllerTest < ActionDispatch::IntegrationTest
  test "a bare permit keeps the listed keys and reads them indifferently" do
    get "/finds", params: { q: "rails", who: "Alice", page: "2", what: { stories: "1", bogus: "x" }, extra: "y" }
    assert_response :success
    assert_select "p#q", "<rails>"
    assert_select "p#other", "one"
    assert_select "p#log", "Alice|1||2|1"
    assert_select "a#next[href=?]", "/finds/page/2?q=rails"
    assert_select "a#later[href=?]", "/finds/page/3?q=rails"
    assert_select "a#strict[href=?]", "/finds/page/4?q=rails"
  end

  test "a partial an action renders links against that action" do
    get "/finds/pager", params: { q: "rails" }
    assert_response :success
    assert_includes response.body, 'href="/finds/pager?page=3&amp;q=rails"'
    get "/finds/helper_pager", params: { q: "rails" }
    assert_response :success
    assert_includes response.body, 'href="/finds/helper_pager?page=3&amp;q=rails"'
  end

  test "absent keys take the fallbacks" do
    get "/finds"
    assert_response :success
    assert_select "p#q", "<>"
    assert_select "p#log", "(All)|||1|2"
  end
end
"#;

/// `params.permit(:q)` handed to a plain class, `@x ||= params.permit(:who,
/// what: %i[...])` read with String and Symbol keys, a multi-local
/// assignment and a `case` over ivars in the template, and a relation ivar
/// joined by association name; each answers what Rails 8.1 answers.
#[test]
fn a_bare_permit_runs_after_emission() {
    contract::overlay()
        .write("test/controllers/finds_controller_test.rb", TEST)
        .run_test("test/controllers/finds_controller_test.rb")
        .assert_passes();
}

/// A subclass reading an inherited nested-permit helper by Symbol key
/// reads its String-keyed storage too.
#[test]
fn an_inherited_filtered_helper_is_read_indifferently() {
    let (emitted, errors) = super::emit_and_run::real_blog()
        .edit(
            "app/controllers/application_controller.rb",
            "  stale_when_importmap_changes\n",
            "  stale_when_importmap_changes\n\n  private\n\n  def log_params\n    params.permit(:who, what: %i[stories])\n  end\n",
        )
        .edit(
            "app/controllers/articles_controller.rb",
            "    @articles = Article.includes(:comments).order(created_at: :desc)\n",
            "    @articles = Article.includes(:comments).order(created_at: :desc)\n    @who = log_params[:who]\n    @stories = log_params.dig(:what, :stories)\n",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let controller = std::fs::read_to_string(emitted.join("app/controllers/articles_controller.rb")).expect("controller");
    assert!(controller.contains("log_params[\"who\"]"), "{controller}");
    assert!(controller.contains("log_params.dig(\"what\", \"stories\")"), "{controller}");
}

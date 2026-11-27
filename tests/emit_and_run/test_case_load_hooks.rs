//! An initializer's `ActiveSupport.on_load(:active_support_test_case)` and
//! `on_load(:action_dispatch_integration_test)` blocks run on the test
//! bases as Rails loads them, so the helpers they define answer in every
//! test of that kind. Ignored as a fork spec until the compiler does; the
//! test passes unchanged in the generated blog on Rails 8.1.4
//! (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/articles_hooks_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "config/initializers/test_helpers.rb",
            r##"ActiveSupport.on_load(:active_support_test_case) do
  def shelf_label
    "oak"
  end
end

ActiveSupport.on_load(:action_dispatch_integration_test) do
  def articles_heading
    "Articles"
  end
end
"##,
        )
        .write(
            FILE,
            r##"require "test_helper"

class ArticlesHooksControllerTest < ActionDispatch::IntegrationTest
  test "the hooks' helpers answer" do
    assert_equal "oak", shelf_label
    get articles_url
    assert_response :success
    assert_includes response.body, articles_heading
  end
end
"##,
        )
}

#[test]
#[ignore = "fork spec: test framework"]
fn an_initializers_test_case_load_hooks_run() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: test framework"]
fn an_initializers_test_case_load_hooks_run_on_spinel() {
    app().run_spin_test("test/articles_hooks_controller_test.rb").assert_passes();
}

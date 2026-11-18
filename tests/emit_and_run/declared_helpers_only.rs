//! With `config.action_controller.include_all_helpers = false` there is
//! no `helper :all`: a view reaches its own controller's default helper
//! and the modules the controller's chain declares with `helper`, not
//! every app helper, as Rails' `_helpers` resolves them. Two helpers
//! define one name, and each controller's view renders its own. Ignored as
//! a fork spec until the compiler does; the test passes unchanged in the
//! generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/bins_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/application.rb",
            "    config.load_defaults 8.1\n",
            "    config.load_defaults 8.1\n    config.action_controller.include_all_helpers = false\n",
        )
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/bins\", to: \"bins#index\"\n  get \"/crates\", to: \"crates#index\"\n  get \"/totes\", to: \"totes#index\"\n",
        )
        .write("app/helpers/bins_helper.rb", "module BinsHelper\n  def shelf_label\n    \"bin label\"\n  end\nend\n")
        .write("app/helpers/totes_helper.rb", "module TotesHelper\n  def shelf_label\n    \"tote label\"\n  end\nend\n")
        .write("app/controllers/bins_controller.rb", "class BinsController < ApplicationController\n  def index\n  end\nend\n")
        .write(
            "app/controllers/crates_controller.rb",
            "class CratesController < ApplicationController\n  helper BinsHelper\n\n  def index\n  end\nend\n",
        )
        .write("app/controllers/totes_controller.rb", "class TotesController < ApplicationController\n  def index\n  end\nend\n")
        .write("app/views/bins/index.html.erb", "<p id=\"label\"><%= shelf_label %></p>\n")
        .write("app/views/crates/index.html.erb", "<p id=\"label\"><%= shelf_label %></p>\n")
        .write("app/views/totes/index.html.erb", "<p id=\"label\"><%= shelf_label %></p>\n")
        .write(
            FILE,
            r#"require "test_helper"

class BinsControllerTest < ActionDispatch::IntegrationTest
  test "each view reaches its own controller's helper" do
    get "/bins"
    assert_includes response.body, "<p id=\"label\">bin label</p>"
    get "/totes"
    assert_includes response.body, "<p id=\"label\">tote label</p>"
  end

  test "a view reaches the helper its controller declares" do
    get "/crates"
    assert_includes response.body, "<p id=\"label\">bin label</p>"
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: views helpers"]
fn without_helper_all_a_view_reaches_its_controllers_helpers() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: views helpers"]
fn without_helper_all_a_view_reaches_its_controllers_helpers_on_spinel() {
    app().run_spin_test("test/bins_controller_test.rb").assert_passes();
}

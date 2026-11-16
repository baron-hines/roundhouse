//! Without `helper :all`, a view resolves a helper name the way Rails
//! builds its controller's `_helpers` module: the controller's
//! `helper_method`s first, then its `helper` modules with the last
//! declared on top and its default `<Name>Helper` lowest, then its
//! parent's; a subclass's own default helper sits above what it inherits,
//! and `clear_helpers` drops the parent's modules but keeps its
//! `helper_method`s; it drops the gems' helpers too, so that controller
//! renders with `layout false`. Ignored as a fork spec until the compiler
//! does; the test passes unchanged in the generated blog on Rails 8.1.4
//! (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/sheds_controller_test.rb";

const VIEW: &str = "<p id=\"tones\"><%= [tone, finish, hue].join(\"|\") %></p>\n";

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
            "  root \"articles#index\"\n  get \"/sheds\", to: \"sheds#index\"\n  get \"/annexes\", to: \"annexes#index\"\n  get \"/barns\", to: \"barns#index\"\n",
        )
        .write(
            "app/helpers/sheds_helper.rb",
            "module ShedsHelper\n  def tone\n    \"sheds tone\"\n  end\n\n  def finish\n    \"sheds finish\"\n  end\n\n  def hue\n    \"sheds hue\"\n  end\nend\n",
        )
        .write("app/helpers/matte_helper.rb", "module MatteHelper\n  def finish\n    \"matte\"\n  end\n\n  def hue\n    \"matte hue\"\n  end\nend\n")
        .write("app/helpers/gloss_helper.rb", "module GlossHelper\n  def finish\n    \"gloss\"\n  end\nend\n")
        .write("app/helpers/annexes_helper.rb", "module AnnexesHelper\n  def hue\n    \"annex hue\"\n  end\nend\n")
        .write("app/helpers/barns_helper.rb", "module BarnsHelper\n  def finish\n    \"barn finish\"\n  end\n\n  def hue\n    \"barn hue\"\n  end\nend\n")
        .write(
            "app/controllers/sheds_controller.rb",
            r#"class ShedsController < ApplicationController
  helper MatteHelper, GlossHelper
  helper_method :tone

  def index
  end

  private

  def tone
    "controller tone"
  end
end
"#,
        )
        .write("app/controllers/annexes_controller.rb", "class AnnexesController < ShedsController\nend\n")
        .write("app/controllers/barns_controller.rb", "class BarnsController < ShedsController\n  clear_helpers\n  layout false\nend\n")
        .write("app/views/sheds/index.html.erb", VIEW)
        .write("app/views/annexes/index.html.erb", VIEW)
        .write("app/views/barns/index.html.erb", VIEW)
        .write(
            FILE,
            r#"require "test_helper"

class ShedsControllerTest < ActionDispatch::IntegrationTest
  test "helper_method, then the last declared module, then the default helper" do
    get "/sheds"
    assert_includes response.body, "<p id=\"tones\">controller tone|gloss|matte hue</p>"
  end

  test "a subclass's default helper sits above what it inherits" do
    get "/annexes"
    assert_includes response.body, "<p id=\"tones\">controller tone|gloss|annex hue</p>"
  end

  test "clear_helpers drops the parent's modules and keeps its helper_methods" do
    get "/barns"
    assert_includes response.body, "<p id=\"tones\">controller tone|barn finish|barn hue</p>"
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: views helpers"]
fn without_helper_all_a_view_resolves_helpers_in_rails_order() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: views helpers"]
fn without_helper_all_a_view_resolves_helpers_in_rails_order_on_spinel() {
    app().run_spin_test("test/sheds_controller_test.rb").assert_passes();
}

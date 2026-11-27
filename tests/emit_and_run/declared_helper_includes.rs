//! The modules ApplicationController names with `helper`, and the modules
//! those include, are the templates' helpers, as Rails' view context has
//! them, though none sits in `app/helpers`: a template's bare `ui_card`
//! and `ui_stack`, defined in modules a declared module includes, render.
//! Ignored as a fork spec until the compiler does; the test passes
//! unchanged in the generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/showcases_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/showcase\", to: \"showcases#show\"\n",
        )
        .edit(
            "app/controllers/application_controller.rb",
            "  allow_browser versions: :modern\n",
            "  allow_browser versions: :modern\n  helper Widgets::Kit\n",
        )
        .write(
            "app/components/widgets/card.rb",
            r#"module Widgets
  module Card
    def ui_card(title, &block)
      content_tag(:section, content_tag(:h2, title) + capture(&block), class: "card")
    end
  end
end
"#,
        )
        .write(
            "app/components/widgets/stack.rb",
            "module Widgets\n  module Stack\n    def ui_stack(items)\n      safe_join(items.map { |item| content_tag(:li, item) })\n    end\n  end\nend\n",
        )
        .write("app/components/widgets/kit.rb", "module Widgets\n  module Kit\n    include Card\n    include Stack\n  end\nend\n")
        .write("app/controllers/showcases_controller.rb", "class ShowcasesController < ApplicationController\n  def show\n  end\nend\n")
        .write(
            "app/views/showcases/show.html.erb",
            "<%= ui_card(\"Tools\") do %><ul><%= ui_stack([\"awl\", \"plane\"]) %></ul><% end %>\n",
        )
        .write(
            FILE,
            r#"require "test_helper"

class ShowcasesControllerTest < ActionDispatch::IntegrationTest
  test "a declared module's included modules are the template's helpers" do
    get "/showcase"
    assert_response :success
    assert_includes response.body, "<section class=\"card\"><h2>Tools</h2><ul><li>awl</li><li>plane</li></ul></section>"
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: views helpers"]
fn modules_a_declared_helper_includes_are_the_templates_helpers() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: views helpers"]
fn modules_a_declared_helper_includes_are_the_templates_helpers_on_spinel() {
    app().run_spin_test("test/showcases_controller_test.rb").assert_passes();
}

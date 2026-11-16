//! A helper's `capture(builder, &block)` yields its arguments to the
//! block, as Rails' `capture(*args)` does: a template's
//! `<%= panel do |p| %>` receives the builder the helper handed in.
//! Ignored as a fork spec until the compiler does; the test passes
//! unchanged in the generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/easels_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/easel\", to: \"easels#show\"\n",
        )
        .write(
            "app/models/panel_builder.rb",
            "class PanelBuilder\n  def initialize(tint)\n    @tint = tint\n  end\n\n  def tint\n    @tint\n  end\nend\n",
        )
        .write(
            "app/helpers/easels_helper.rb",
            r#"module EaselsHelper
  def panel(tint, &block)
    content_tag(:div, capture(PanelBuilder.new(tint), &block), class: "panel")
  end
end
"#,
        )
        .write("app/controllers/easels_controller.rb", "class EaselsController < ApplicationController\n  def show\n  end\nend\n")
        .write("app/views/easels/show.html.erb", "<%= panel(\"ochre\") do |p| %><b><%= p.tint %></b><% end %>\n")
        .write(
            FILE,
            r#"require "test_helper"

class EaselsControllerTest < ActionDispatch::IntegrationTest
  test "capture yields the builder to the template's block" do
    get "/easel"
    assert_response :success
    assert_includes response.body, "<div class=\"panel\"><b>ochre</b></div>"
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: views helpers"]
fn capture_yields_its_arguments_to_the_block() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: views helpers"]
fn capture_yields_its_arguments_to_the_block_on_spinel() {
    app().run_spin_test("test/easels_controller_test.rb").assert_passes();
}

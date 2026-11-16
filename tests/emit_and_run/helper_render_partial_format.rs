//! A helper renders a partial it is given by name in another format, as
//! Rails' view does: it saves `lookup_context.formats`, renders
//! `render(partial: name, formats: [:text])` inside a block `content_tag`,
//! and restores the formats, so the page's own render of the same partial
//! still finds its html template. The text partial's output is not
//! escaped (ERB leaves `text/plain` unescaped); the html one's is.
//! Ignored as a fork spec until the compiler does; the test passes
//! unchanged in the generated blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/trays_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/trays\", to: \"trays#index\"\n",
        )
        .write(
            "app/helpers/trays_helper.rb",
            r#"module TraysHelper
  def template_script(name)
    saved = lookup_context.formats
    content_tag(:script, type: "text/template") do
      render(partial: name, formats: [:text])
    end
  ensure
    lookup_context.formats = saved
  end
end
"#,
        )
        .write("app/controllers/trays_controller.rb", "class TraysController < ApplicationController\n  def index\n    @tray = \"Seeds & bulbs\"\n  end\nend\n")
        .write("app/views/trays/_slot.text.erb", "{{label}} <%= @tray %>\n")
        .write("app/views/trays/_slot.html.erb", "<b><%= @tray %></b>\n")
        .write("app/views/trays/index.html.erb", "<%= template_script(\"trays/slot\") %>\n<%= render \"trays/slot\" %>\n")
        .write(
            FILE,
            r#"require "test_helper"

class TraysControllerTest < ActionDispatch::IntegrationTest
  test "a helper renders a partial in another format" do
    get "/trays"
    assert_response :success
    assert_includes response.body, "<script type=\"text/template\">{{label}} Seeds & bulbs\n</script>"
    assert_includes response.body, "<b>Seeds &amp; bulbs</b>"
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: views helpers"]
fn a_helper_renders_a_partial_in_another_format() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: views helpers"]
fn a_helper_renders_a_partial_in_another_format_on_spinel() {
    app().run_spin_test("test/trays_controller_test.rb").assert_passes();
}

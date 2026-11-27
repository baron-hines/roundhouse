//! A helper that renders a partial answers the partial's String, as
//! ActionView's `render` does. lobsters' `ApplicationHelper#link_post`
//! renders `helpers/_link_post`, and `ApplicationController` includes the
//! helper; typed with the controller as `self`, the call was the
//! controller's `render` (nil). Spinel enforces the sidecar, so every
//! `link_post(...).to_s` came out empty: no logout button on /settings.

use super::emit_and_run;
use roundhouse::project::BuildTarget;

const HELPER: &str = r##"module PostHelper
  def link_post(button_label, link)
    render partial: "helpers/link_post", locals: { button_label: button_label, link: link }
  end
end
"##;

const PARTIAL: &str = "<form action=\"<%= link %>\" method=\"post\"><%= button_label %></form>\n";

fn overlay(included: bool) -> emit_and_run::Overlay {
    let o = emit_and_run::real_blog()
        .write("app/helpers/post_helper.rb", HELPER)
        .write("app/views/helpers/_link_post.html.erb", PARTIAL)
        .edit(
            "app/views/articles/show.html.erb",
            "<% content_for :title, \"Showing article\" %>\n",
            "<% content_for :title, \"Showing article\" %>\n<%= link_post(\"Go\", \"/go\") %>\n",
        );
    if included {
        o.edit(
            "app/controllers/application_controller.rb",
            "class ApplicationController < ActionController::Base\n",
            "class ApplicationController < ActionController::Base\n  include PostHelper\n",
        )
    } else {
        o
    }
}

#[test]
fn a_helper_render_is_the_partial_string() {
    for included in [false, true] {
        let (emitted, errors) = overlay(included).emit(BuildTarget::Spinel);
        assert!(errors.is_empty(), "{errors:#?}");
        let sig = std::fs::read_to_string(emitted.join("app/models/post_helper.rbs")).expect("post_helper.rbs");
        assert!(
            sig.contains("def self.link_post: (String button_label, String link) -> String"),
            "included={included}\n{sig}"
        );
    }
}

//! A view's url Hash that names a controller and no action, or a nil
//! action, links to that controller's index route, as Rails' route set
//! normalizes the options (`action ||= "index"`); one that names an
//! action links to it. The test below passes unchanged in the generated
//! blog on Rails 8.1 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/shortcuts_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/shortcut\", to: \"shortcuts#index\"\n  get \"/shortcut/links\", to: \"shortcuts#links\"\n  get \"/shortcut/away\", to: \"shortcuts#away\"\n",
        )
        .write(
            "app/controllers/shortcuts_controller.rb",
            r#"class ShortcutsController < ApplicationController
  def index
  end

  def links
  end

  def away
  end
end
"#,
        )
        .write("app/views/shortcuts/index.html.erb", "<p>index</p>\n")
        .write("app/views/shortcuts/away.html.erb", "<p>away</p>\n")
        .write(
            "app/views/shortcuts/links.html.erb",
            r#"<nav>
  <%= link_to "Start", { controller: "shortcuts" }, id: "start" %>
  <%= link_to "Again", { controller: "shortcuts", action: nil }, id: "again" %>
  <%= link_to "Away", { controller: "shortcuts", action: "away" }, id: "away" %>
  <%= link_to "Merged", { controller: "shortcuts" }.merge(action: "away"), id: "merged" %>
  <%= link_to "Over nil", { controller: "shortcuts", action: nil }.merge(action: "away"), id: "over-nil" %>
  <%= link_to "Merged start", { controller: "shortcuts" }.merge(page: 2), id: "merged-start" %>
</nav>
"#,
        )
        .write(
            FILE,
            r##"require "test_helper"

class ShortcutsControllerTest < ActionDispatch::IntegrationTest
  test "a controller without an action is its index" do
    get "/shortcut/links"
    assert_response :success
    assert_select "a#start[href=?]", "/shortcut"
    assert_select "a#again[href=?]", "/shortcut"
    assert_select "a#away[href=?]", "/shortcut/away"
    assert_select "a#merged[href=?]", "/shortcut/away"
    assert_select "a#over-nil[href=?]", "/shortcut/away"
    assert_select "a#merged-start[href=?]", "/shortcut?page=2"
  end
end
"##,
        )
}

#[test]
fn a_controller_without_an_action_is_its_index() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "requires spin and spinel on PATH plus SQLite development libraries"]
fn a_controller_without_an_action_is_its_index_on_spinel() {
    let (emitted, errors) = app().emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    let file = "test/shortcuts_controller_test.rb";
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

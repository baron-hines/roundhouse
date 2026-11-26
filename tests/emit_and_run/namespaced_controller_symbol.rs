//! A namespaced controller and a top-level one whose names flatten alike
//! dispatch to themselves. lobsters has both `Mod::MailsController`
//! (`/mod/mails`) and `ModMailsController` (`/mod_mails`); the router
//! named both `:mod_mails`, so every /mod/mails page reached the top-level
//! controller and answered an empty 200. A `params.expect(:id)` handed to
//! a String column's condition stays a String (`find_by(short_id:
//! params.expect(:id))`), where the id reading made it 0.

use super::emit_and_run;

const TEST_PATH: &str = "test/controllers/admin_notes_controller_test.rb";

#[test]
fn flattened_alike_controllers_dispatch_to_themselves() {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "    resources :comments, only: [:create, :destroy]\n  end\n",
            concat!(
                "    resources :comments, only: [:create, :destroy]\n  end\n",
                "  namespace :admin do\n    resources :notes, only: :index\n  end\n",
                "  resources :admin_notes, only: :index\n",
                "  get \"/by_title/:id\" => \"admin_notes#by_title\"\n",
            ),
        )
        .write(
            "app/controllers/admin/notes_controller.rb",
            "class Admin::NotesController < ApplicationController\n  def index\n    render plain: \"namespaced\"\n  end\nend\n",
        )
        .write(
            "app/controllers/admin_notes_controller.rb",
            concat!(
                "class AdminNotesController < ApplicationController\n",
                "  def index\n    render plain: \"top-level\"\n  end\n\n",
                "  def by_title\n    article = Article.find_by(title: params.expect(:id))\n    render plain: article ? article.title : \"none\"\n  end\n",
                "end\n",
            ),
        )
        .write(
            TEST_PATH,
            r#"require "test_helper"

class AdminNotesControllerTest < ActionDispatch::IntegrationTest
  test "each controller answers its own route" do
    get "/admin/notes"
    assert_equal "namespaced", response.body
    get "/admin_notes"
    assert_equal "top-level", response.body
  end

  test "an expected key for a String column stays a String" do
    Article.create!(title: "hello", body: "A body long enough for validation.")
    get "/by_title/hello"
    assert_equal "hello", response.body
  end
end
"#,
        )
        .run_test(TEST_PATH)
        .assert_passes();
}

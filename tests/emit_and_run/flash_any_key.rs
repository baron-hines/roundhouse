//! A flash key other than notice/alert is kept, carried across a redirect
//! and shown once, as Rails' FlashHash does. lobsters sets `flash[:error]`
//! and `flash[:success]` (over a hundred sites) and its layout shows them;
//! the runtime's Flash held notice and alert only, so every one of those
//! messages vanished on both trees. `redirect_to …, flash: { … }` sets
//! them too.

use super::emit_and_run;

const TEST_PATH: &str = "test/controllers/flashes_controller_test.rb";

#[test]
fn any_flash_key_is_carried_and_shown_once() {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "    resources :comments, only: [:create, :destroy]\n  end\n",
            concat!(
                "    resources :comments, only: [:create, :destroy]\n  end\n",
                "  get \"/flashes/set\" => \"flashes#set\"\n",
                "  get \"/flashes/kw\" => \"flashes#kw\"\n",
                "  get \"/flashes/now\" => \"flashes#now\"\n",
                "  get \"/flashes/show\" => \"flashes#show\"\n",
            ),
        )
        .write(
            "app/controllers/flashes_controller.rb",
            concat!(
                "class FlashesController < ApplicationController\n",
                "  def set\n    flash[:error] = \"Bad thing\"\n    redirect_to \"/flashes/show\"\n  end\n\n",
                "  def kw\n    redirect_to \"/flashes/show\", flash: { success: \"Done\" }\n  end\n\n",
                "  def now\n    flash.now[:success] = \"Right now\"\n    render plain: flash[:success]\n  end\n\n",
                "  def show\n    render plain: [flash[:error], flash[:success]].compact.join(\"|\")\n  end\n",
                "end\n",
            ),
        )
        .write(
            TEST_PATH,
            r#"require "test_helper"

class FlashesControllerTest < ActionDispatch::IntegrationTest
  test "a key set before a redirect shows once on the next page" do
    get "/flashes/set"
    assert_redirected_to "/flashes/show"
    follow_redirect!
    assert_equal "Bad thing", response.body
    get "/flashes/show"
    assert_equal "", response.body
  end

  test "redirect_to takes a flash hash" do
    get "/flashes/kw"
    follow_redirect!
    assert_equal "Done", response.body
  end

  test "flash.now is this request's only" do
    get "/flashes/now"
    assert_equal "Right now", response.body
    get "/flashes/show"
    assert_equal "", response.body
  end
end
"#,
        )
        .run_test(TEST_PATH)
        .assert_passes();
}

//! A view helper module keeps a singleton method beside the instance
//! method of the same name, as Ruby does: the view calls the instance
//! `seal(text)`, which calls the module's own `self.seal(prefix, text)`,
//! and other code calls the singleton directly. Ignored as a fork spec
//! until the compiler does; the test passes unchanged in the generated
//! blog on Rails 8.1.4 (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/controllers/vaults_controller_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/vault\", to: \"vaults#show\"\n",
        )
        .write(
            "app/helpers/seals_helper.rb",
            r##"module SealsHelper
  class << self
    def seal(prefix, text)
      "#{prefix}:#{text.reverse}"
    end
  end

  def seal(text)
    SealsHelper.seal("view", text)
  end
end
"##,
        )
        .write("app/controllers/vaults_controller.rb", "class VaultsController < ApplicationController\n  def show\n  end\nend\n")
        .write("app/views/vaults/show.html.erb", "<p id=\"seal\"><%= seal(\"abc\") %></p>\n")
        .write(
            FILE,
            r#"require "test_helper"

class VaultsControllerTest < ActionDispatch::IntegrationTest
  test "the view calls the instance method and the module keeps its singleton" do
    get "/vault"
    assert_response :success
    assert_includes response.body, "<p id=\"seal\">view:cba</p>"
    assert_equal "test:zyx", SealsHelper.seal("test", "xyz")
  end
end
"#,
        )
}

#[test]
#[ignore = "fork spec: views helpers"]
fn a_helper_modules_singleton_method_stays_beside_its_instance_method() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: views helpers"]
fn a_helper_modules_singleton_method_stays_beside_its_instance_method_on_spinel() {
    app().run_spin_test("test/vaults_controller_test.rb").assert_passes();
}

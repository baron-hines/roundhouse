//! #2293: a splatted key list on a one-key lookup of a `params.permit(...)` result.
//!
//! `fetch(*%w[title fallback])` passes two arguments, the key and the default, so Rails answers the default when
//! the key is absent; `fetch(*keys)` the same through a variable; `key?(*%w[body])` passes the one key. The
//! lowered `Params.permitted` Hash takes them as Ruby's Hash does, so each splat stays a list of arguments,
//! its String keys named as the Hash's Symbol keys, not one key made of the whole list.

use super::emit_and_run;

const ROUTES: &str = r#"  post "/splat_keys/fetch_literal" => "splat_keys#fetch_literal"
  post "/splat_keys/fetch_variable" => "splat_keys#fetch_variable"
  post "/splat_keys/fetch_present" => "splat_keys#fetch_present"
  post "/splat_keys/key_literal" => "splat_keys#key_literal"
"#;

const CONTROLLER: &str = r#"class SplatKeysController < ApplicationController
  def fetch_literal
    render plain: params.permit(%w[body]).fetch(*%w[title fallback]).inspect
  end

  def fetch_variable
    keys = %w[title fallback]
    render plain: params.permit(%w[body]).fetch(*keys).inspect
  end

  def fetch_present
    render plain: params.permit(%w[body]).fetch(*%w[body fallback]).inspect
  end

  def key_literal
    render plain: params.permit(%w[body]).key?(*%w[body]).inspect
  end
end
"#;

const TEST: &str = r#"require "test_helper"

class SplatKeysControllerTest < ActionDispatch::IntegrationTest
  PARAMS = { title: "t", body: "long enough body" }

  test "a splatted literal list on fetch gives the default for an absent key" do
    post "/splat_keys/fetch_literal", params: PARAMS
    assert_equal "\"fallback\"", response.body
  end

  test "a splatted list variable on fetch gives the default for an absent key" do
    post "/splat_keys/fetch_variable", params: PARAMS
    assert_equal "\"fallback\"", response.body
  end

  test "a splatted literal list on fetch reads a permitted key" do
    post "/splat_keys/fetch_present", params: PARAMS
    assert_equal "\"long enough body\"", response.body
  end

  test "a splatted literal list on key? asks for its one key" do
    post "/splat_keys/key_literal", params: PARAMS
    assert_equal "true", response.body
  end
end
"#;

#[test]
fn a_splatted_key_list_on_a_permitted_hash_stays_a_list() {
    emit_and_run::real_blog()
        .edit("config/routes.rb", "  root \"articles#index\"\n", &format!("  root \"articles#index\"\n{ROUTES}"))
        .write("app/controllers/splat_keys_controller.rb", CONTROLLER)
        .write("test/controllers/splat_keys_controller_test.rb", TEST)
        .run_test("test/controllers/splat_keys_controller_test.rb")
        .assert_passes();
}

/// The Spinel emit writes the same arguments: the list's first element is the Symbol key, the rest as written.
#[test]
fn the_spinel_emit_keeps_a_splatted_key_list() {
    let (tree, errors) = emit_and_run::real_blog()
        .edit("config/routes.rb", "  root \"articles#index\"\n", &format!("  root \"articles#index\"\n{ROUTES}"))
        .write("app/controllers/splat_keys_controller.rb", CONTROLLER)
        .emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    let src = std::fs::read_to_string(tree.join("app/controllers/splat_keys_controller.rb")).expect("controller");
    for line in [
        ".fetch(:title, \"fallback\")",
        ".fetch(keys[0].to_sym, *keys.drop(1))",
        ".fetch(:body, \"fallback\")",
        ".key?(:body)",
    ] {
        assert!(src.contains(line), "missing `{line}`:\n{src}");
    }
}

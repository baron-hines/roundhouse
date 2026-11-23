//! #2279: String key names on a `params.permit(...)` result.
//!
//! A top-level `permit` lowers to a Symbol-keyed `Params.permitted` chain,
//! where Rails answers an `ActionController::Parameters`, which takes a
//! String key as it takes a Symbol. So `.except("version")` and
//! `.slice("title")` on it, and on what `merge` / `compact` answer from it,
//! name the Symbol keys, as do calls on a no-argument private helper that
//! answers such a `permit` (`def x_params = params.permit(%w[a b])`). A
//! plain Hash the app builds, inline or in a helper, keeps Ruby's answer,
//! as do a helper the class overrides with one, a helper with a `return`,
//! a memoized helper, and a splatted key list on a one-key method.

use super::emit_and_run;

const ROUTES: &str = r#"  post "/imports/splat" => "imports#splat"
  post "/imports/string" => "imports#string"
  post "/imports/symbol" => "imports#symbol"
  post "/imports/slice" => "imports#slice"
  post "/imports/rename" => "imports#rename"
  post "/imports/variable" => "imports#variable"
  post "/imports/splat_variable" => "imports#splat_variable"
  post "/imports/merge_string" => "imports#merge_string"
  post "/imports/plain" => "imports#plain"
  post "/imports/helper" => "imports#helper"
  post "/imports/plain_helper" => "imports#plain_helper"
  post "/imports/early_return" => "imports#early_return"
  post "/imports/memo_helper" => "imports#memo_helper"
  post "/imports/fetch_splat" => "imports#fetch_splat"
  post "/override_imports/override" => "override_imports#override"
"#;

const CONTROLLER: &str = r#"class ImportsController < ApplicationController
  skip_forgery_protection

  def splat
    attrs = params.permit(%w[title body version]).merge(body: "x").compact.except(*%w[version])
    render plain: attrs.keys.map(&:to_s).sort.join(",")
  end

  def string
    attrs = params.permit(%w[title body version]).except("version")
    render plain: attrs.keys.map(&:to_s).sort.join(",")
  end

  def symbol
    attrs = params.permit(%w[title body version]).except(:version)
    render plain: attrs.keys.map(&:to_s).sort.join(",")
  end

  def slice
    attrs = params.permit(%w[title body version]).slice("title")
    render plain: attrs.keys.map(&:to_s).sort.join(",")
  end

  # Only the body may change here; the stored title is kept.
  def rename
    article = Article.create!(title: "kept", body: "long enough body")
    article.update!(params.permit(%w[title body]).except("title"))
    render plain: article.reload.title
  end

  def variable
    key = "version"
    attrs = params.permit(%w[title body version]).except(key)
    render plain: attrs.keys.map(&:to_s).sort.join(",")
  end

  def splat_variable
    keys = %w[version]
    attrs = params.permit(%w[title body version]).except(*keys)
    render plain: attrs.keys.map(&:to_s).sort.join(",")
  end

  def merge_string
    attrs = params.permit(%w[title body]).merge("title" => "z")
    render plain: attrs.keys.map(&:to_s).sort.join(",")
  end

  # A Hash the app builds is not Parameters: Ruby's except leaves a Symbol key a String does not name.
  def plain
    attrs = { title: params[:title], version: params[:version] }.except("version")
    render plain: attrs.keys.map(&:to_s).sort.join(",")
  end

  # Only the body may change here; the stored title is kept.
  def helper
    article = Article.create!(title: "kept", body: "long enough body")
    article.update!(article_params.except("title"))
    render plain: article.reload.title
  end

  def plain_helper
    attrs = plain_attrs.except("version")
    render plain: attrs.keys.map(&:to_s).sort.join(",")
  end

  def early_return
    render plain: early_attrs.fetch("title")
  end

  def memo_helper
    @memo_attrs = { "title" => "kept" }
    render plain: memo_attrs.fetch("title")
  end

  # Emit-only (no runtime case): the emitters cast a splatted first key as one key, on main too.
  def fetch_splat
    render plain: params.permit(%w[body]).fetch(*%w[title fallback]).inspect
  end

  private

  def article_params = params.permit(%w[title body])

  def early_attrs
    return { "title" => "kept" } if params[:early]
    params.permit(%w[title])
  end

  def memo_attrs
    @memo_attrs ||= params.permit(%w[title])
  end

  def plain_attrs
    { title: params[:title], version: params[:version] }
  end
end
"#;

/// Overrides the permit helper `article_params` it inherits with a plain String-keyed Hash.
const OVERRIDE: &str = r#"class OverrideImportsController < ImportsController
  def override
    render plain: article_params.fetch("title")
  end

  private

  def article_params
    { "title" => "kept" }
  end
end
"#;

const TEST: &str = r#"require "test_helper"

class ImportsControllerTest < ActionDispatch::IntegrationTest
  PARAMS = { title: "t", body: "long enough body", version: "1" }

  test "except with a splatted String list drops the key" do
    post "/imports/splat", params: PARAMS
    assert_equal "body,title", response.body
  end

  test "except with a String drops the key" do
    post "/imports/string", params: PARAMS
    assert_equal "body,title", response.body
  end

  test "except with a Symbol drops the key" do
    post "/imports/symbol", params: PARAMS
    assert_equal "body,title", response.body
  end

  test "slice with a String keeps the key" do
    post "/imports/slice", params: PARAMS
    assert_equal "title", response.body
  end

  test "a key except removed is not written" do
    post "/imports/rename", params: PARAMS
    assert_equal "kept", response.body
  end

  test "except with a String variable drops the key" do
    post "/imports/variable", params: PARAMS
    assert_equal "body,title", response.body
  end

  test "except with a splatted String list variable drops the key" do
    post "/imports/splat_variable", params: PARAMS
    assert_equal "body,title", response.body
  end

  test "merge with a String key replaces the permitted key" do
    post "/imports/merge_string", params: PARAMS
    assert_equal "body,title", response.body
  end

  test "a plain Hash keeps Ruby's except" do
    post "/imports/plain", params: PARAMS
    assert_equal "title,version", response.body
  end

  test "except with a String on a permit helper drops the key" do
    post "/imports/helper", params: PARAMS
    assert_equal "kept", response.body
  end

  test "a helper's plain Hash keeps Ruby's except" do
    post "/imports/plain_helper", params: PARAMS
    assert_equal "title,version", response.body
  end

  test "a helper overridden with a plain Hash keeps its String key" do
    post "/override_imports/override", params: PARAMS
    assert_equal "kept", response.body
  end

  test "a helper that returns a plain Hash early keeps its String key" do
    post "/imports/early_return", params: PARAMS.merge(early: "1")
    assert_equal "kept", response.body
  end

  test "a memoized helper keeps the String key of the Hash it holds" do
    post "/imports/memo_helper", params: PARAMS
    assert_equal "kept", response.body
  end

end
"#;

#[test]
fn string_keys_on_a_permitted_hash_name_its_keys() {
    emit_and_run::real_blog()
        .edit("config/routes.rb", "  root \"articles#index\"\n", &format!("  root \"articles#index\"\n{ROUTES}"))
        .write("app/controllers/imports_controller.rb", CONTROLLER)
        .write("app/controllers/override_imports_controller.rb", OVERRIDE)
        .write("test/controllers/imports_controller_test.rb", TEST)
        .run_test("test/controllers/imports_controller_test.rb")
        .assert_passes();
}

/// The Spinel emit writes the same Symbol keys, through a helper too; a plain Hash keeps its String, as do a
/// helper this class overrides, one with a `return`, a memoized one, and a splatted key list on `fetch`.
#[test]
fn the_spinel_emit_names_the_symbol_keys() {
    let (tree, errors) = emit_and_run::real_blog()
        .edit("config/routes.rb", "  root \"articles#index\"\n", &format!("  root \"articles#index\"\n{ROUTES}"))
        .write("app/controllers/imports_controller.rb", CONTROLLER)
        .write("app/controllers/override_imports_controller.rb", OVERRIDE)
        .emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    let src = std::fs::read_to_string(tree.join("app/controllers/imports_controller.rb")).expect("controller");
    for line in [
        ".merge(body: \"x\").compact.except(*%i[version])",
        ".except(:title))",
        ".slice(:title)",
        ".except(key.to_sym)",
        ".except(*keys.map { |x| x.to_sym })",
        ".merge(title: \"z\")",
        "}.except(\"version\")",
        "article_params.except(:title)",
        "plain_attrs.except(\"version\")",
        "early_attrs.fetch(\"title\")",
        "memo_attrs.fetch(\"title\")",
        ".fetch(*%w[title fallback]",
    ] {
        assert!(src.contains(line), "missing `{line}`:\n{src}");
    }
    let child = std::fs::read_to_string(tree.join("app/controllers/override_imports_controller.rb")).expect("child");
    assert!(child.contains("article_params.fetch(\"title\")"), "{child}");
}

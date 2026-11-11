//! #1989: each element of `params.require(:list)` is an
//! `ActionController::Parameters` in Rails 8.1.4, so `permit`, `expect` and
//! Symbol reads work on it. The bodies and answers below are that version's.
//! The nested keys are Strings, as a JSON body's are: the in-process test
//! request keeps a Ruby Hash's own keys.
//! A refusal raises out of `post` in this test (#1519); Rails answers 400.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

const CONTROLLER: &str = r#"class ItemsController < ActionController::API
  def permit_only
    render json: { rows: params.require(:list).map { |item| item.permit(:name, :price) } }
  end

  def expect_permit
    rows = params.require(:list).map { |item| item.expect(:name); item.permit(:name, :price) }
    render json: { rows: rows }
  end

  def implicit_it
    rows = params.require(:list).map { it.expect(:name); it.permit(:name, :price) }
    render json: { rows: rows }
  end

  def expect_value
    render json: { rows: params.require(:list).map { |item| item.expect(:name) } }
  end

  def nested_filter
    render json: { rows: params.require(:list).map { |item| item.permit(:name, tags: [], meta: [:a]) } }
  end

  def element_reads
    render json: { rows: params.require(:list).map { |item| [item[:name], item["name"], item.key?(:name), item.fetch(:price, 0)] } }
  end

  def fetch_block
    render json: { rows: params.require(:list).map { |item| item.fetch(:price) { 0 } } }
  end

  def shadowed
    render json: { rows: params.require(:list).map { |item| [1].map { |item| item + 1 } } }
  end
end
"#;

const TEST: &str = r#"require "test_helper"

class ItemsControllerTest < ActionDispatch::IntegrationTest
  ROWS = { list: [{ "name" => "a", "price" => 3, "admin" => true }, { "name" => "b" }] }
  PERMITTED = { rows: [{ name: "a", price: 3 }, { name: "b" }] }.to_json

  test "permit keeps the listed scalars" do
    post "/items/permit_only", params: ROWS, as: :json
    assert_equal PERMITTED, response.body
  end

  test "permit drops a nested value under a scalar key" do
    post "/items/permit_only", params: { list: [{ "name" => { "deep" => 1 } }] }, as: :json
    assert_equal({ rows: [{}] }.to_json, response.body)
  end

  test "expect then permit" do
    post "/items/expect_permit", params: ROWS, as: :json
    assert_equal PERMITTED, response.body
  end

  test "an implicit it parameter" do
    post "/items/implicit_it", params: ROWS, as: :json
    assert_equal PERMITTED, response.body
  end

  test "expect answers the value" do
    post "/items/expect_value", params: ROWS, as: :json
    assert_equal({ rows: ["a", "b"] }.to_json, response.body)
  end

  test "expect keeps false and zero" do
    post "/items/expect_value", params: { list: [{ "name" => false }, { "name" => 0 }] }, as: :json
    assert_equal({ rows: [false, 0] }.to_json, response.body)
  end

  test "expect refuses a missing key" do
    assert_raises(ActionController::ParameterMissing) { post "/items/expect_value", params: { list: [{ "name" => "a" }, { "price" => 3 }] }, as: :json }
  end

  test "expect refuses a blank string" do
    assert_raises(ActionController::ParameterMissing) { post "/items/expect_permit", params: { list: [{ "name" => "" }] }, as: :json }
  end

  test "expect refuses a nested value" do
    assert_raises(ActionController::ParameterMissing) { post "/items/expect_permit", params: { list: [{ "name" => { "deep" => 1 } }] }, as: :json }
  end

  test "nested filters" do
    post "/items/nested_filter", params: { list: [{ "name" => "a", "tags" => ["x", 1], "meta" => { "a" => "1", "b" => "2" }, "admin" => true }] }, as: :json
    assert_equal({ rows: [{ name: "a", tags: ["x", 1], meta: { a: "1" } }] }.to_json, response.body)
  end

  test "Symbol reads are indifferent" do
    post "/items/element_reads", params: ROWS, as: :json
    assert_equal({ rows: [["a", "a", true, 3], ["b", "b", true, 0]] }.to_json, response.body)
  end

  test "fetch with a block finds the value" do
    post "/items/fetch_block", params: ROWS, as: :json
    assert_equal({ rows: [3, 0] }.to_json, response.body)
  end

  test "a nested block's own parameter is left alone" do
    post "/items/shadowed", params: ROWS, as: :json
    assert_equal({ rows: [[2], [2]] }.to_json, response.body)
  end
end
"#;

#[test]
fn elements_of_a_required_list_answer_as_rails_does() {
    let routes: String = ["permit_only", "expect_permit", "implicit_it", "expect_value", "nested_filter", "element_reads", "fetch_block", "shadowed"]
        .iter()
        .map(|a| format!("  post \"/items/{a}\", to: \"items#{a}\"\n"))
        .collect();
    emit_and_run::real_blog()
        .write("app/controllers/items_controller.rb", CONTROLLER)
        .edit("config/routes.rb", "  root \"articles#index\"\n", &format!("  root \"articles#index\"\n{routes}"))
        .write("test/controllers/items_controller_test.rb", TEST)
        .run_test("test/controllers/items_controller_test.rb")
        .assert_passes();
}

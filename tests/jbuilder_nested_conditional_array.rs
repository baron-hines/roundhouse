//! A `json.<key> do … end` whose only statement is a conditional
//! `json.array!`. The array is the key's value when the condition holds,
//! and the key is left out when it does not (`_merge_block` answers
//! BLANK, `_set_value` skips the key).
//!
//! Rails 8.1.4 + jbuilder 2.15.1 answers the three rows below; an empty
//! collection is a value, `[]`. The Spinel side is
//! `support/small_rails_gaps_app.rs`.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const APPLICATION_RECORD: &str = "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n";

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"widgets\", force: :cascade do |t|\n    t.string \"name\"\n    t.string \"email\"\n  end\nend\n",
        )
        .write("config/routes.rb", "Rails.application.routes.draw do\n  resources :widgets, only: :show do\n    get :nils, on: :member\n  end\nend\n")
        .write("app/models/application_record.rb", APPLICATION_RECORD)
        .write(
            "app/models/widget.rb",
            "class Widget < ApplicationRecord\n  def codes\n    name.to_s.split(\",\")\n  end\n\n  def codes_ready?\n    !email.nil?\n  end\n\n  def calls\n    @calls ||= 0\n  end\n\n  def maybe_codes\n    @calls = calls + 1\n    email == \"none\" ? nil : codes\n  end\n\n  def maybe_items\n    @calls = calls + 1\n    email == \"none\" ? nil : [self]\n  end\nend\n",
        )
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::API\nend\n")
        .write(
            "app/controllers/widgets_controller.rb",
            "class WidgetsController < ApplicationController\n  def show\n    @widget = Widget.find(params[:id])\n  end\n\n  def nils\n    @widget = Widget.find(params[:id])\n    @widgets = @widget.maybe_items\n  end\nend\n",
        )
        .write("app/views/widgets/_code.json.jbuilder", "json.v code\n")
        .write(
            "app/views/widgets/nils.json.jbuilder",
            r#"json.plain do
  json.array!(@widget.maybe_codes) if @widget.codes_ready?
end
json.blocks do
  json.array!(@widget.maybe_codes) { |c| json.v c } if @widget.codes_ready?
end
json.parts do
  json.array!(@widget.maybe_codes, partial: "widgets/code", as: :code) if @widget.codes_ready?
end
json.extracted do
  json.array!(@widgets, :name) if @widget.codes_ready?
end
json.calls @widget.calls
"#,
        )
        .write(
            "app/views/widgets/show.json.jbuilder",
            r#"json.codes do
  json.array!(@widget.codes) if @widget.codes_ready?
end
json.name @widget.name
json.others do
  json.array!(@widget.codes) { |c| json.v c } unless @widget.codes_ready?
end
json.tail 1
"#,
        )
}

const SCRIPT: &str = r##"
require_relative "app/controllers/widgets_controller"
def render_for(widget)
  controller = WidgetsController.new
  controller.params = { "id" => widget.id.to_s, "format" => "json" }
  controller.process_action(:show)
  controller.body
end
ready = Widget.create!(name: "x,y", email: "e")
got = render_for(ready)
raise "ready: #{got}" unless got == %q({"codes":["x","y"],"name":"x,y","tail":1})
pending = Widget.create!(name: "x,y")
got = render_for(pending)
raise "pending: #{got}" unless got == %q({"name":"x,y","others":[{"v":"x"},{"v":"y"}],"tail":1})
empty = Widget.create!(name: "", email: "e")
got = render_for(empty)
raise "empty: #{got}" unless got == %q({"codes":[],"name":"","tail":1})
puts "conditional array passed"
"##;

#[test]
fn a_conditional_array_is_the_keys_value_or_leaves_the_key_out() {
    app().run_ruby(SCRIPT).assert_passes();
}

const NILS_SCRIPT: &str = r##"
require_relative "app/controllers/widgets_controller"
def render_nils(widget)
  controller = WidgetsController.new
  controller.params = { "id" => widget.id.to_s, "format" => "json" }
  controller.process_action(:nils)
  controller.body
end
none = Widget.create!(name: "x,y", email: "none")
got = render_nils(none)
want = %q({"plain":[],"blocks":[],"parts":[],"extracted":[],"calls":4})
raise "nil collections: #{got}" unless got == want
some = Widget.create!(name: "x,y", email: "e")
got = render_nils(some)
want = %q({"plain":["x","y"],"blocks":[{"v":"x"},{"v":"y"}],"parts":[{"v":"x"},{"v":"y"}],"extracted":[{"name":"x,y"}],"calls":4})
raise "with collections: #{got}" unless got == want
puts "nil collections passed"
"##;

#[test]
fn a_nil_collection_is_an_empty_array_and_reads_once() {
    app().run_ruby(NILS_SCRIPT).assert_passes();
}

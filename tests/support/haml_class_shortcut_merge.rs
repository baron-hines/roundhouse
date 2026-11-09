//! One HAML shortcut-class + hash `class:` merge page shared by the
//! interpreted lane (`tests/haml_class_shortcut_merge.rs`) and the native
//! one (`tests/spinel_toolchain/haml_class_shortcut_merge.rs`).
use super::emit_and_run;

/// A `WidgetsController#index` page exercising every merge shape the
/// compiler supports: a scalar (String) value, a false conditional, a
/// ternary, and an Array literal — plus an attribute-order probe and
/// the two byte-identical (no hash `class:` at all) guard elements.
pub fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table \"widgets\", force: :cascade do |t|\n    t.string \"name\"\n  end\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\n  get \"/widgets\", to: \"widgets#index\"\nend\n")
        .write("app/controllers/widgets_controller.rb", r#"class WidgetsController < ApplicationController
  def index
    @key = "alert"
    @flag = params[:flag] == "1"
    @other = params[:other] == "1"
  end
end
"#)
        .write(
            "app/views/widgets/index.html.haml",
            ".gadget{ class: @key }\n\
             .gadget{ class: ('gadget--rtl' if @flag) }\n\
             .gadget{ class: @flag ? 'on' : 'off' }\n\
             .gadget{ class: [@flag && 'gadget--hot', @other && 'gadget--cold'] }\n\
             %a.gadget{ href: '/w', class: @key, title: 't' }\n\
             .gadget{ title: 't' }\n\
             .gadget\n",
        )
}

pub fn render(flag: &str, other: &str) -> String {
    r#"
require_relative "app/controllers/widgets_controller"
controller = WidgetsController.new
controller.params = {"#.to_string()
        + &format!(r#""flag" => {flag:?}, "other" => {other:?}"#)
        + r#"}
controller.process_action(:index)
raise "status #{controller.status}" unless controller.status == 200
puts controller.body
"#
}

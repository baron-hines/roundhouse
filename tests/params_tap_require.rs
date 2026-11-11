//! `params.require(:widget).permit(:a).tap { |p| p.require(:a) }` calls
//! `require` on the synthesized params class. The class had no such
//! method, so the call reached the private `Kernel#require`.
//!
//! Rails 8.1.4: an absent or blank value raises `ParameterMissing`, a
//! present one is returned. The Spinel side is
//! `support/small_rails_gaps_app.rs`.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema.define do\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\n  resources :widgets, only: :create\nend\n")
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::API\nend\n")
        .write(
            "app/controllers/widgets_controller.rb",
            r#"class WidgetsController < ApplicationController
  def create
    values = widget_params
    render plain: values[:email].to_s
  end

  private

  def widget_params
    params.require(:widget).permit(:name, :email).tap { |p| p.require(:email) }
  end
end
"#,
        )
}

const SCRIPT: &str = r#"
require_relative "app/controllers/widgets_controller"
def answer(body)
  controller = WidgetsController.new
  controller.params = { "widget" => body }
  begin
    controller.process_action(:create)
    "ok:" + controller.body
  rescue ActionController::ParameterMissing => e
    "missing:" + e.param.to_s
  end
end
got = answer({ "name" => "n", "email" => "e@x" })
raise "present: #{got}" unless got == "ok:e@x"
got = answer({ "name" => "n" })
raise "absent: #{got}" unless got == "missing:email"
got = answer({ "name" => "n", "email" => "" })
raise "blank: #{got}" unless got == "missing:email"
puts "params require passed"
"#;

#[test]
fn a_required_key_of_a_permitted_params_object_raises_when_blank() {
    app().run_ruby(SCRIPT).assert_passes();
}

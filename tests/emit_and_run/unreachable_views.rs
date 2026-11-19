//! A template no request reaches has its errors reported as warnings
//! (lobsters' `mod/notes/index`, whose ivars no action sets); it is still
//! emitted. One an action, a route, a render or an inheriting controller
//! reaches keeps its errors.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/controllers/notes_controller.rb", "class NotesController < ApplicationController\n  def create\n    head :ok\n  end\n\n  def shown\n    render :rendered\n  end\nend\n")
        .write("app/controllers/archived_notes_controller.rb", "class ArchivedNotesController < ::NotesController\n  def inherited\n  end\nend\n")
        .write("app/views/notes/orphan.html.erb", "<p><%= @missing.title %></p>\n")
        .write("app/views/notes/rendered.html.erb", "<p>rendered</p>\n")
        .write("app/views/notes/routed.html.erb", "<p>routed</p>\n")
        .write("app/views/notes/inherited.html.erb", "<p>inherited</p>\n")
        .edit("config/routes.rb", "Rails.application.routes.draw do\n", "Rails.application.routes.draw do\n  get \"notes/routed\", to: \"notes#routed\"\n")
}

#[test]
fn an_unreachable_template_s_errors_are_warnings() {
    let warnings = app().ingest_warnings();
    assert!(warnings.iter().any(|w| w.contains("`notes/orphan` is rendered by no action, route or render call")), "{warnings:#?}");
    for kept in ["notes/rendered", "notes/routed", "notes/inherited"] {
        assert!(!warnings.iter().any(|w| w.contains(&format!("`{kept}` is rendered by no action"))), "{kept}: {warnings:#?}");
    }
    let (_emitted, errors) = app().emit(roundhouse::project::BuildTarget::Ruby);
    assert!(!errors.iter().any(|e| e.contains("@missing")), "{errors:#?}");
    // The same read in a template an action renders stays an error.
    let (_emitted, errors) = app()
        .write("app/views/notes/rendered.html.erb", "<p><%= @missing.title %></p>\n")
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("@missing")), "{errors:#?}");
}

#[test]
fn the_app_with_it_still_boots() {
    app().run_ruby("raise \"kept\" unless defined?(Views::Notes) && Views::Notes.respond_to?(:rendered) && Views::Notes.respond_to?(:orphan)\nputs \"boots\"\n").assert_passes();
}

/// maybe's `Notifiable`: an ancestor's `view_context.render(**notification)`
/// renders the partials its hashes name, not a template, when no hash the
/// controller builds holds `template:`/`action:`/`file:` (an attribute hash
/// passed to `create!` is not one), and `include Pagy::Backend` defines no
/// action of the template's name. A `template:` hash keeps the render open.
#[test]
fn a_splatted_partial_render_and_pagy_backend_reach_no_template() {
    let notifiable = "module Notifiable\n  extend ActiveSupport::Concern\n\n  private\n    def render_notices\n      notices.map { |n| view_context.render(**n) }\n    end\n\n    def notices\n      [{ partial: \"shared/notice\", locals: { message: \"hi\" } }]\n    end\n\n    def log_visit\n      Rails.logger.info(controller: controller_name, action: action_name)\n    end\nend\n";
    let base = |notifiable: &str| {
        app()
            .write("app/controllers/concerns/notifiable.rb", notifiable)
            .write("app/views/shared/_notice.html.erb", "<p><%= message %></p>\n")
            .edit("Gemfile.lock", "    debug (1.11.1)\n", "    pagy (9.3.5)\n    debug (1.11.1)\n")
            .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  pagy\n")
            .edit(
                "app/controllers/application_controller.rb",
                "class ApplicationController < ActionController::Base\n",
                "class ApplicationController < ActionController::Base\n  include Notifiable\n  include Pagy::Backend\n",
            )
    };
    let warnings = base(notifiable).ingest_warnings();
    assert!(warnings.iter().any(|w| w.contains("`notes/orphan` is rendered by no action, route or render call")), "{warnings:#?}");
    for open in [
        notifiable.replace("{ partial: \"shared/notice\"", "{ template: \"notes/orphan\""),
        notifiable.replace("[{ partial: \"shared/notice\", locals: { message: \"hi\" } }]", "[{}.merge(template: \"notes/orphan\")]"),
        notifiable.replace("notices.map { |n| view_context.render(**n) }", "view_context.render(**params.permit(:template).to_h.symbolize_keys)"),
    ] {
        let warnings = base(&open).ingest_warnings();
        assert!(!warnings.iter().any(|w| w.contains("`notes/orphan` is rendered by no action")), "{open}: {warnings:#?}");
    }
}

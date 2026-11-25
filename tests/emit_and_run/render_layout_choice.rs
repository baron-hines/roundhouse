//! `render action: "new", layout: !request.xhr?` (lobsters' story preview): the page in the default layout, and the
//! bare template for an XHR, whose `X-Requested-With` Rails matches anywhere in the header and in any case.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write(
            "app/controllers/notes_controller.rb",
            "class NotesController < ApplicationController\n  def preview\n    render action: \"new\", layout: !request.xhr?\n  end\nend\n",
        )
        .write("app/views/layouts/application.html.erb", "<main><%= yield %></main>\n")
        .write("app/views/notes/new.html.erb", "<p>form</p>\n")
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/note.rb", "class Note < ApplicationRecord\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\n  get \"/notes/preview\" => \"notes#preview\"\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema[8.1].define(version: 2026_01_01_000000) do\n  create_table \"notes\", force: :cascade do |t|\n    t.string \"body\"\n  end\nend\n",
        )
}

#[test]
fn a_negated_layout_option_picks_the_layout_per_request() {
    app()
        .run_ruby(
            r##"page = ->(env) do
  status, _headers, body = Main.run_rack({ "REQUEST_METHOD" => "GET", "PATH_INFO" => "/notes/preview", "QUERY_STRING" => "", "rack.input" => StringIO.new("") }.merge(env))
  raise "GET /notes/preview answered #{status}" unless status == 200
  text = +""
  body.each { |part| text << part }
  text
end
full = page.({})
raise "a page request renders in the layout: #{full.inspect}" unless full.include?("<main><p>form</p>")
bare = page.({ "HTTP_X_REQUESTED_WITH" => "xmlhttprequest" })
raise "an XHR renders the bare template: #{bare.inspect}" unless bare.include?("<p>form</p>") && !bare.include?("<main>")
puts "layout choice passed"
"##,
        )
        .assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_negated_layout_option_picks_the_layout_per_request_on_spinel() {
    app()
        .run_spinel(
            r##"def preview(xhr)
  request = ActionDispatch::Request.new
  request.request_method = "GET"
  request.path = "/notes/preview"
  request.query_string = ""
  request.headers["X-Requested-With"] = "xmlhttprequest" if xhr
  ActionController::Current.request = request
  controller = NotesController.new
  controller.request = request
  ActionController::Current.controller = controller
  controller.request_method = "GET"
  controller.params = {}
  controller.process_action(:preview)
  controller.body
end
full = preview(false)
raise "a page request renders in the layout: #{full}" unless full.include?("<main><p>form</p>")
bare = preview(true)
raise "an XHR renders the bare template: #{bare}" unless bare.include?("<p>form</p>") && !bare.include?("<main>")
puts "layout choice passed"
"##,
        )
        .assert_passes();
}

/// Not rewritten where the `if` would read the condition before an argument that changes it: the call stays a
/// computed layout, reported.
#[test]
fn a_layout_condition_an_argument_assigns_stays_an_error() {
    let (_tree, errors) = app()
        .write(
            "app/controllers/notes_controller.rb",
            "class NotesController < ApplicationController\n  def preview\n    xhr = false\n    render action: (xhr = true; \"new\"), layout: !xhr\n  end\nend\n",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("`render layout:` given a computed value")), "{errors:#?}");
}

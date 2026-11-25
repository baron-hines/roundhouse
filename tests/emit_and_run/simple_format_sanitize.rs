//! `simple_format(text)` with Rails' default `sanitize: true` (lobsters' story text): the text is sanitized, then split
//! into paragraphs. `EXPECTED` is what Rails 8.1.4's `ActionView::Base.empty` answers for the same three calls.

use super::emit_and_run;

const TEXT: &str = "Hi <b>bold</b> <script>alert(1)</script><a href=\\\"javascript:x\\\" onclick=\\\"y\\\">l</a>\\n\\nnext & <i>it</i>\\nline";

const EXPECTED: &str = "<p>Hi <b>bold</b> alert(1)<a>l</a></p>\n\n<p>next &amp; <i>it</i>\n<br />line</p>|<div class=\"c\">Hi <b>bold</b> alert(1)<a>l</a></div>\n\n<div class=\"c\">next &amp; <i>it</i>\n<br />line</div>|<p></p>";

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write(
            "app/controllers/notes_controller.rb",
            &format!("class NotesController < ApplicationController\n  def show\n    @text = \"{TEXT}\"\n  end\nend\n"),
        )
        .write(
            "app/views/notes/show.html.erb",
            "<%= simple_format(@text) %>|<%= simple_format(@text, { class: \"c\" }, wrapper_tag: \"div\") %>|<%= simple_format(\"\") %>",
        )
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/note.rb", "class Note < ApplicationRecord\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\n  get \"/note\" => \"notes#show\"\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema[8.1].define(version: 2026_01_01_000000) do\n  create_table \"notes\", force: :cascade do |t|\n    t.string \"body\"\n  end\nend\n",
        )
}

#[test]
fn a_default_simple_format_sanitizes_as_rails_does() {
    app()
        .run_ruby(&format!(
            r##"status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/note", "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
raise "GET /note answered #{{status}}" unless status == 200
text = +""
body.each {{ |part| text << part }}
raise "simple_format: #{{text.inspect}}" unless text.include?({EXPECTED:?})
puts "simple_format passed"
"##
        ))
        .assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_default_simple_format_sanitizes_as_rails_does_on_spinel() {
    app()
        .run_spinel(&format!(
            r##"controller = NotesController.new
controller.request_method = "GET"
controller.params = {{}}
controller.process_action(:show)
raise "simple_format: #{{controller.body}}" unless controller.body.include?({EXPECTED:?})
puts "simple_format passed"
"##
        ))
        .assert_passes();
}

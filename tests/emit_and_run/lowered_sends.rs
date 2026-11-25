//! lobsters' calls that checked clean and then stopped its Ruby emit, each answering as Rails does once emitted:
//! a `try(:[], :vote)` on an untyped value and a `content_tag(…, aria: …)` (sends a lowering builds after analysis,
//! once read as failed dispatches), `rand(0..9999).days.ago`, a record's `changes`, and an `after_action` whose bare
//! `mail` is the message the action built (`ApplicationMailer#check_email_blocklist`).

use super::emit_and_run;

const NOTE: &str = r#"class Note < ApplicationRecord
  attr_accessor :current_vote

  def flagged?
    current_vote.try(:[], :vote) == -1
  end

  def self.backdated
    rand(0..9).days.ago
  end
end
"#;

const MAILERS: &str = r#"class ApplicationMailer < ActionMailer::Base
  default from: "nobody@example.com"
  after_action :check_blocklist

  def check_blocklist
    if mail.to[0] == "blocked@example.com"
      mail.perform_deliveries = false
    end
  end
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("app/helpers/application_helper.rb", "module ApplicationHelper\n  def divider_tag\n    content_tag(:span, \" | \", aria: {hidden: \"true\"})\n  end\nend\n")
        .write("app/controllers/notes_controller.rb", "class NotesController < ApplicationController\n  def index\n  end\nend\n")
        .write("app/views/notes/index.html.erb", "<%= divider_tag %>")
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/note.rb", NOTE)
        .write("app/mailers/application_mailer.rb", MAILERS)
        .write(
            "app/mailers/note_mailer.rb",
            "class NoteMailer < ApplicationMailer\n  def ping(to)\n    mail(to: to, subject: \"ping\")\n  end\nend\n",
        )
        .write("app/views/note_mailer/ping.text.erb", "ping\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\n  resources :notes, only: :index\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema[8.1].define(version: 2026_01_01_000000) do\n  create_table \"notes\", force: :cascade do |t|\n    t.string \"title\"\n  end\nend\n",
        )
}

const SCRIPT: &str = r##"note = Note.new(title: "a")
raise "untyped try: #{note.flagged?.inspect}" unless note.flagged? == false
note.current_vote = { vote: -1 }
raise "try(:[], :vote)" unless note.flagged?
at = Note.backdated
raise "rand(0..9).days.ago: #{at.inspect}" unless at.is_a?(Time) && at <= Time.now && at >= Time.now - 10 * 86400
raise "changes: #{note.changes.inspect}" unless note.changes["title"] == [nil, "a"] && note.changed == ["title"]
sent = NoteMailer.ping("ann@example.com").deliver_now
blocked = NoteMailer.ping("blocked@example.com").deliver_now
raise "after_action's mail: #{sent.perform_deliveries.inspect} #{blocked.perform_deliveries.inspect}" unless sent.perform_deliveries && !blocked.perform_deliveries
puts "lowered sends passed"
"##;

#[test]
fn calls_a_lowering_rebuilds_answer_as_rails_does() {
    let page = r##"status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/notes", "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
text = +""
body.each { |part| text << part }
raise "content_tag aria: #{text.inspect}" unless status == 200 && text.include?(%(<span aria-hidden="true"> | </span>))
"##;
    app().run_ruby(&format!("{page}{SCRIPT}")).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn calls_a_lowering_rebuilds_answer_as_rails_does_on_spinel() {
    app().run_spinel_with_db(SCRIPT).assert_passes();
}

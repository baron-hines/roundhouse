//! A belongs_to's scope applies over its target's default scope wherever
//! the association is read: the polymorphic reader, its preload, a plain
//! reader and a join, so a scope that unscopes the default condition reaches the rows
//! the default scope hides, as Rails builds the association scope on the
//! target's `default_scoped` relation. `Comment`'s polymorphic association
//! has no inverse `as:`, so its reader reads any model, as Rails'
//! constantize does. The expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/project.rb", r##"class Project < ApplicationRecord
  default_scope { where(archived: false) }
end
"##)
        .write("app/models/comment.rb", r##"class Comment < ApplicationRecord
  belongs_to :topic, -> { unscope(where: :archived) }, polymorphic: true, optional: true
end
"##)
        .write("app/models/task.rb", r##"class Task < ApplicationRecord
  belongs_to :project, -> { unscope(where: :archived) }, optional: true
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    live = Project.create!(name: "live")
    old = Project.create!(name: "old", archived: true)
    Comment.create!(body: "1", topic: live)
    Comment.create!(body: "2", topic_type: "Project", topic_id: old.id)
    Task.create!(title: "a", project: live)
    Task.create!(title: "b", project_id: old.id)
    # Rails: the archived project is reached every way
    check("join", Task.joins(:project).order(:title).pluck(:title) == ["a", "b"])
    check("reader", Task.order(:title).map { |t| t.project && t.project.name } == ["live", "old"])
    check("polymorphic reader", Comment.order(:body).map { |c| c.topic && c.topic.name } == ["live", "old"])
    check("polymorphic preload", Comment.order(:body).preload(:topic).map { |c| c.topic && c.topic.name } == ["live", "old"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "projects", force: :cascade do |t|
    t.string "name"
    t.boolean "archived", default: false
  end
  create_table "comments", force: :cascade do |t|
    t.string "body"
    t.string "topic_type"
    t.integer "topic_id"
  end
  create_table "tasks", force: :cascade do |t|
    t.string "title"
    t.integer "project_id"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_belongs_to_scope_applies_over_the_targets_default_scope() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_belongs_to_scope_applies_over_the_targets_default_scope_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

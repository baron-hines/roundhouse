//! An app class macro that declares `belongs_to :account` and a
//! `before_validation` filling it still runs at class load when a later
//! `belongs_to :account` of the model replaces its association: the
//! callback is registered and fills the key, as Ruby runs each class body
//! statement and Rails' add_reflection keeps the later reflection.
//! Ignored as a fork spec until the compiler does; the expected values
//! are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true

  def self.copies_account_from(source)
    belongs_to :account
    before_validation do
      self.account_id ||= public_send(source)&.account_id
    end
  end
end
"##)
        .write("app/models/account.rb", r##"class Account < ApplicationRecord
end
"##)
        .write("app/models/project.rb", r##"class Project < ApplicationRecord
  belongs_to :account
end
"##)
        .write("app/models/task.rb", r##"class Task < ApplicationRecord
  belongs_to :project
  copies_account_from :project
  belongs_to :account, inverse_of: false
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    account = Account.create!(name: "acme")
    project = Project.create!(account: account, name: "roof")
    task = Task.new(project: project, title: "tiles")
    check("the macro's callback fills the key", task.valid? && task.account_id == account.id)
    task.save!
    check("saved with it", Task.find(task.id).account == account)
    check("the later reflection is kept", Task.reflect_on_association(:account).options[:inverse_of] == false)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "accounts", force: :cascade do |t|
    t.string "name"
  end
  create_table "projects", force: :cascade do |t|
    t.integer "account_id"
    t.string "name"
  end
  create_table "tasks", force: :cascade do |t|
    t.integer "account_id"
    t.integer "project_id"
    t.string "title"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: ruby semantics"]
fn superseded_macro_association_still_registers_its_callback() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn superseded_macro_association_still_registers_its_callback_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

//! `record.destroy` and `destroy!` answer the record itself, as Rails'
//! `Persistence#destroy` does: a model method reads on the result
//! (`result.ip_address` after `if result = user.destroy`, as discourse's
//! `UserDestroyer` writes it).

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/member.rb", "class Member < ApplicationRecord\n  def label\n    \"#{name}@#{ip_address}\"\n  end\nend\n")
        .write("app/models/probe.rb", r##"module Probe
  def self.run
    member = Member.create!(name: "ann", ip_address: "10.0.0.1")
    if result = member.destroy
      raise result.ip_address.inspect unless result.ip_address == "10.0.0.1"
      raise result.label.inspect unless result.label == "ann@10.0.0.1"
      raise "still persisted" if result.persisted?
    end
    other = Member.create!(name: "bo", ip_address: "10.0.0.2")
    raise other.destroy!.label unless other.destroy!.label == "bo@10.0.0.2"
  end
end
"##)
        .write("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table \"members\", force: :cascade do |t|\n    t.string \"name\"\n    t.string \"ip_address\"\n  end\nend\n")
}

#[test]
fn destroy_answers_the_record_itself() {
    let errors = app().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
    let run = app().run_ruby("Probe.run\nputs \"destroy probe passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("destroy probe passed"));
}

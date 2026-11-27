//! A model inside a namespace prepending a Concern of that namespace by
//! its relative name (`prepend Audit` for `Ns::Audit` in `Ns::Post`):
//! the `prepended` block runs on the model and the Concern's class
//! methods reach it, as Ruby resolves the name from the model's nesting.
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/ns/audit.rb", r##"module Ns
  module Audit
    extend ActiveSupport::Concern

    prepended do
      before_save :stamp
    end

    class_methods do
      def audit_label
        "audited #{name}"
      end
    end

    def stamp
      self.title = "#{title}!"
    end
  end
end
"##)
        .write("app/models/ns/post.rb", r##"module Ns
  class Post < ApplicationRecord
    self.table_name = "posts"
    prepend Audit
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  # Rails: ["t!", "audited Ns::Post"]
  def self.namespaced_prepend
    post = Ns::Post.create!(title: "t")
    [post.title, Ns::Post.audit_label]
  end

  def self.run
    Ns::Post.delete_all
    check("namespaced_prepend", namespaced_prepend == ["t!", "audited Ns::Post"])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "posts", force: :cascade do |t|
    t.string "title"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_namespaced_prepend_reaches_its_prepender() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_namespaced_prepend_reaches_its_prepender_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

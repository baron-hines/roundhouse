//! `validates ..., on: SIGNUP`, where `SIGNUP = :create` is a constant of
//! the class body, runs in that context, beside an `on: :update` rule, as
//! Ruby reads the constant before Rails sees the option. Ignored as a
//! fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/vendor.rb", r##"class Vendor < ApplicationRecord
  SIGNUP = :create

  validates :email, :phone, presence: true, on: SIGNUP
  validates :name, presence: true, on: :update
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    v = Vendor.new(name: "", email: "", phone: "")
    check("a create checks email and phone", v.save == false && v.errors.attribute_names == [:email, :phone])
    v.email = "a@b.c"
    v.phone = "555"
    check("and passes once filled", v.save == true)
    v.email = ""
    check("an update checks name, not email", !v.valid? && v.errors.full_messages == ["Name can't be blank"])
    v.name = "acme"
    check("and passes", v.save == true)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "vendors", force: :cascade do |t|
    t.string "name"
    t.string "email"
    t.string "phone"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: validations"]
fn validates_on_a_context_constant_runs_in_that_context() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: validations"]
fn validates_on_a_context_constant_runs_in_that_context_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

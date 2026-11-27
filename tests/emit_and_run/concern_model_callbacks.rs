//! A concern whose `included` block extends `ActiveModel::Callbacks` and
//! defines model callbacks on a plain Active Model includer: the
//! includer's `run_callbacks` runs the hooks the block registered.
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/concerns/sendable.rb", r##"module Sendable
  extend ActiveSupport::Concern

  included do
    extend ActiveModel::Callbacks
    define_model_callbacks :delivery
    before_delivery :seal
    after_delivery :stamp
  end

  def deliver
    run_callbacks(:delivery) { steps << "sent" }
  end

  def steps
    @steps ||= []
  end

  private

  def seal
    steps << "sealed"
  end

  def stamp
    steps << "stamped"
  end
end
"##)
        .write("app/models/letter.rb", r##"class Letter
  include ActiveModel::Model
  include Sendable
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  # Rails: ["sealed", "sent", "stamped"]
  def self.model_callbacks
    letter = Letter.new
    letter.deliver
    letter.steps
  end

  def self.run
    check("model_callbacks", model_callbacks == ["sealed", "sent", "stamped"])
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_concerns_model_callbacks_run_on_a_plain_includer() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_concerns_model_callbacks_run_on_a_plain_includer_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

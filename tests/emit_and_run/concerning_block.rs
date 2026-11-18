//! `concerning :Name do … end` in a model body, as Active Support's
//! `Module#concerning` defines it: a Concern module under the model,
//! included at once, whose `included` block and `class_methods` reach the
//! model beside its plain methods.
//! The probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/kettle.rb", r##"class Kettle < ApplicationRecord
  BOIL = 100

  concerning :Heating do
    included do
      scope :hot, -> { where("degrees >= ?", 90) }
    end

    class_methods do
      def hot_count
        hot.count
      end
    end

    # `BOIL` is the model's: the block was written in `Kettle`'s class
    # body, so that is the lexical scope Ruby resolves it in.
    def boiling?
      degrees >= BOIL
    end
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  # Rails: [2, [false, true], true]
  def self.concerning_block
    Kettle.create!(degrees: 95)
    Kettle.create!(degrees: 100)
    Kettle.create!(degrees: 20)
    [Kettle.hot_count, Kettle.hot.order(:degrees).map(&:boiling?), Kettle.include?(Kettle::Heating)]
  end

  def self.run
    Kettle.delete_all
    check("concerning_block", concerning_block == [2, [false, true], true])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
  create_table "kettles", force: :cascade do |t|
    t.integer "degrees"
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_concerning_block_is_an_included_concern() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_concerning_block_is_an_included_concern_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

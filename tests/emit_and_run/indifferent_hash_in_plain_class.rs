//! A plain class whose Hash is indifferent on one path and Symbol-keyed
//! on another (`@given = given || DEFAULTS`) answers a Symbol key on
//! both, as Rails does: the reads convert the Symbol-keyed one too, as a
//! parameter both reach is converted. The probe's expected values are
//! ActiveRecord 8.1.4's answers.
//!
//! An indifferent Hash a test alone hands to such a class is not among
//! the evidence its parameter is typed from, so its reads there stay
//! Symbol-keyed; that hand-off is ledgered as a warning.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/sifter.rb", r##"class Sifter
  DEFAULTS = { grades: [{ name: "medium" }], mesh: 20 }

  def initialize(given = nil)
    @given = given || DEFAULTS
  end

  def given
    @given
  end

  def labels
    grades = given[:grades]
    grades.map { |grade| grade[:name] }
  end

  def mesh
    given.fetch(:mesh)
  end

  def coarse?
    given.key?(:coarse)
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.sift(sifter)
    [sifter.labels, sifter.mesh, sifter.coarse?]
  end

  # Rails: [["medium"], 20, false]
  def self.defaults
    sift(Sifter.new)
  end

  # Rails: [["fine", "rough"], 40, false]
  def self.given
    given = { "grades" => [{ "name" => "fine" }, { "name" => "rough" }], "mesh" => 40 }.with_indifferent_access
    sift(Sifter.new(given))
  end

  def self.run
    check("defaults", defaults == [["medium"], 20, false])
    check("given", given == [["fine", "rough"], 40, false])
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_hash_indifferent_on_one_path_answers_symbol_keys() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_hash_indifferent_on_one_path_answers_symbol_keys_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
fn an_indifferent_hash_only_a_test_hands_over_is_ledgered() {
    let warnings = emit_and_run::empty_app()
        .write("app/models/strainer.rb", r##"class Strainer
  def initialize(given)
    @given = given
  end

  def size
    @given[:size]
  end
end
"##)
        .write("test/models/strainer_test.rb", r##"require "test_helper"

class StrainerTest < ActiveSupport::TestCase
  test "size" do
    assert_equal 3, Strainer.new({ "size" => 3 }.with_indifferent_access).size
  end
end
"##)
        .lowering_warnings(roundhouse::project::BuildTarget::Ruby);
    assert!(
        warnings.iter().any(|d| d.message.contains("an indifferent Hash handed from a test to `Strainer#initialize`")),
        "{warnings:#?}"
    );
}

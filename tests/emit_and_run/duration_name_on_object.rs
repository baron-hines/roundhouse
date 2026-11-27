//! A Duration-unit name (`minutes`, `seconds`, `weeks`) called on an
//! object whose class defines it calls that method, as Ruby does: only
//! a number's `minutes` builds an `ActiveSupport::Duration`. The
//! receiver here comes from `Array#first`, `Array#find` or a
//! safe-navigation read, which may answer nil, so it types as a union
//! with nil; one is a plain class, one a `Data` value. The
//! probe's expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  self.abstract_class = true
end
"##)
        .write("app/models/lap.rb", r##"class Lap
  attr_reader :minutes, :seconds

  def initialize(minutes, seconds)
    @minutes = minutes
    @seconds = seconds
  end

  def weeks
    [minutes, seconds].sum
  end

  def self.laps
    [Lap.new(3, 20), Lap.new(4, 5)]
  end
end
"##)
        .write("app/models/split.rb", r##"Split = Data.define(:minutes, :seconds)
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  # Rails: [3, 20, 23]
  def self.first_lap
    lap = Lap.laps.first
    [lap.minutes, lap.seconds, lap.weeks]
  end

  # Rails: [4, nil]
  def self.safe_lap
    lap = Lap.laps.last
    none = Lap.laps.find { |l| l.minutes > 9 }
    [lap&.minutes, none&.minutes]
  end

  # Rails: [1, 30]
  def self.first_split
    split = [Split.new(minutes: 1, seconds: 30)].first
    [split.minutes, split.seconds]
  end

  # Rails: 120
  def self.number_still_builds_a_duration
    2.minutes.to_i
  end

  def self.run
    check("first_lap", first_lap == [3, 20, 23])
    check("safe_lap", safe_lap == [4, nil])
    check("first_split", first_split == [1, 30])
    check("number_still_builds_a_duration", number_still_builds_a_duration == 120)
  end
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema.define do
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
fn a_duration_name_on_an_object_calls_its_method() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_duration_name_on_an_object_calls_its_method_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

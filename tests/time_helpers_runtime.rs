//! ActiveSupport::Testing::TimeHelpers on the Ruby family
//! (runtime/spinel/scaffold/ruby_overlay/runtime/active_support_testing_time_helpers.rb,
//! ported in Shopify/roundhouse-1). tests/time_helpers_runtime.rb runs
//! the same observations against native activesupport 8.1.4 and the
//! port and requires them equal. The emitted-tree tests hold the wiring:
//! the CRuby test overlay includes the port in place of the shared
//! harness's offset stand-in, which moved only `Time.now` and
//! `ActiveSupport.now`, so `Date.today` inside `travel_to` answered the
//! real date. A spinel tree keeps the stand-in.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;
use std::path::Path;
use std::process::Command;

#[test]
fn time_helpers_answer_like_activesupport() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("ruby")
        .arg(root.join("tests/time_helpers_runtime.rb"))
        .arg("compare")
        .current_dir(root)
        .output()
        .expect("ruby is on PATH");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stdout.contains("Native ActiveSupport and the emitted TimeHelpers agree"),
        "TimeHelpers diverged from activesupport\n=== stdout ===\n{stdout}\n=== stderr ===\n{stderr}"
    );
    assert!(stdout.lines().count() > 45, "too few observations:\n{stdout}");
}

const CLOCK_TEST: &str = r#"require "test_helper"

class ClockTest < ActiveSupport::TestCase
  def test_freeze_time_holds_the_clock
    freeze_time
    first = Time.now
    sleep 0.01
    assert_equal first, Time.now
    assert_equal 0, first.usec
  end

  def test_travel_to_a_block_and_back
    travel_to Time.utc(2001, 2, 3, 4, 5, 6) do
      assert_equal 981173106, Time.now.to_i
      assert_equal Date.new(2001, 2, 3), Time.now.utc.to_date
    end
    assert Time.now.year > 2001
  end

  def test_travel_moves_from_the_frozen_instant
    travel_to Time.utc(2001, 2, 3, 4, 5, 6)
    travel 60
    assert_equal 981173166, Time.now.to_i
    travel_back
    assert Time.now.year > 2001
  end

  # Source order: the stub this one leaves is gone by the next test, as
  # Minitest's after_teardown leaves it.
  def test_travels_without_coming_back
    travel_to Time.utc(2001, 2, 3, 4, 5, 6)
    assert_equal 2001, Time.now.year
  end

  def test_the_clock_is_real_again
    assert Time.now.year > 2001
  end
end
"#;

// No `freeze_time` anywhere: plain `travel_to` must move the calendar
// too. Noon UTC is the same date in every zone the host may run in.
const CALENDAR_TEST: &str = r#"require "test_helper"

class CalendarTest < ActiveSupport::TestCase
  def test_date_today_follows_travel_to
    travel_to Time.utc(2001, 2, 3, 12, 0, 0) do
      assert_equal Date.new(2001, 2, 3), Date.today
      assert_equal [2001, 2, 3], [DateTime.now.year, DateTime.now.month, DateTime.now.day]
    end
    assert Date.today.year > 2001
  end

  def test_a_record_saved_while_traveling_is_stamped_then
    travel_to Time.utc(2001, 2, 3, 4, 5, 6) do
      probe = Probe.create!(name: "x")
      assert_equal 981173106, probe.created_at.to_i
    end
  end
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n")
        .write("app/models/probe.rb", "class Probe < ApplicationRecord\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table :probes do |t|\n    t.string :name\n    t.timestamps\n  end\nend\n",
        )
        .write("test/models/clock_test.rb", CLOCK_TEST)
        .write("test/models/calendar_test.rb", CALENDAR_TEST)
}

#[test]
fn a_cruby_tree_travels_with_rails_time_helpers() {
    let run = app().run_test("test/models/clock_test.rb");
    run.assert_passes();
    assert!(run.stdout.contains("ClockTest: 5 tests passed"), "{}", run.stdout);
}

#[test]
fn travel_to_moves_the_calendar_and_record_stamps() {
    let run = app().run_test("test/models/calendar_test.rb");
    run.assert_passes();
    assert!(run.stdout.contains("CalendarTest: 2 tests passed"), "{}", run.stdout);
}

#[test]
fn only_the_ruby_family_carries_the_port() {
    let (ruby, errors) = app().emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    assert!(ruby.join("runtime/active_support_testing_time_helpers.rb").exists());
    let support = std::fs::read_to_string(ruby.join("test/test_support_cruby.rb")).unwrap();
    assert!(support.contains("include ActiveSupport::Testing::TimeHelpers"), "{support}");

    let (spinel, errors) = app().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    assert!(!spinel.join("runtime/active_support_testing_time_helpers.rb").exists());
}

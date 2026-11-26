//! `ActiveJob::TestHelper#enqueued_jobs_with` answers the jobs the test
//! adapter still holds, as Rails' test adapter keeps them (a Hash with
//! `:job`, `:args` and `:queue`), narrowed by `only:` and `queue:` and
//! yielded to a block. Ignored as a fork spec until the compiler does; the
//! test passes unchanged in the generated blog on Rails 8.1.4
//! (`bin/rails test`).

use super::emit_and_run;

const FILE: &str = "test/models/shelf_jobs_test.rb";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "app/jobs/dust_shelf_job.rb",
            r##"class DustShelfJob < ApplicationJob
  queue_as :chores

  def perform(shelf)
  end
end
"##,
        )
        .write(
            "app/jobs/count_books_job.rb",
            r##"class CountBooksJob < ApplicationJob
  def perform(shelf)
  end
end
"##,
        )
        .write(
            FILE,
            r##"require "test_helper"

class ShelfJobsTest < ActiveSupport::TestCase
  include ActiveJob::TestHelper

  test "the held jobs, narrowed" do
    DustShelfJob.perform_later("top")
    CountBooksJob.perform_later("low")
    assert_equal [DustShelfJob, CountBooksJob], enqueued_jobs_with.map { |job| job[:job] }
    dusted = enqueued_jobs_with(only: DustShelfJob)
    assert_equal [["top"]], dusted.map { |job| job[:args] }
    assert_equal ["chores"], dusted.map { |job| job[:queue] }
    assert_equal [CountBooksJob], enqueued_jobs_with(queue: "default").map { |job| job[:job] }
    seen = []
    enqueued_jobs_with(only: CountBooksJob) { |job| seen << job[:args] }
    assert_equal [["low"]], seen
  end
end
"##,
        )
}

#[test]
#[ignore = "fork spec: test framework"]
fn enqueued_jobs_with_answers_the_held_jobs() {
    app().run_test(FILE).assert_passes();
}

#[test]
#[ignore = "fork spec: test framework"]
fn enqueued_jobs_with_answers_the_held_jobs_on_spinel() {
    app().run_spin_test("test/shelf_jobs_test.rb").assert_passes();
}

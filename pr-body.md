Main is red in `tests/emit_and_run/sidekiq_push.rs` (the `sidekiq_adapter` tests) since #1677 and #1682 both landed. Five tests fail:

- `perform_later_pushes_the_payload_to_redis`
- `an_after_commit_job_is_pushed_at_commit`
- `set_options_schedule_and_route_the_push`
- `a_job_whose_parameters_are_named_queue_at_and_q_still_pushes_its_arguments`
- `an_app_on_sidekiq_with_no_jobs_gets_no_redis`

real-blog's `Article` and `Comment` declare `broadcasts_to`. On `:sidekiq`, their create and update broadcasts now push `Turbo::Streams::ActionBroadcastJob`, as Rails does. As a result, each recorded push list began with Article's broadcast before the test's own job. The "no jobs" app also has jobs now, so it gets a Redis client.

The runtime behavior is right; the tests' setup is out of date. The tests that record their own job's push, or need an app with no jobs, now start from real-blog with those two `broadcasts_to` lines taken out. The broadcast push itself is covered by `tests/broadcast_job.rs`. `strict_targets_warn_once_and_emit_the_same_tree` gets the same base on both sides of its tree comparison.

Test only, no runtime change. `cargo test --release --test emit_and_run sidekiq_adapter` passes 21/21 (5 failed on main), and `--test broadcast_job` passes 6/6.

🤖 Generated with [Claude Code](https://claude.com/claude-code)

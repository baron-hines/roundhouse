<!-- claude-projects-attribution: {"github_login":"baron-hines"} -->

Fixes #1461. A job whose `perform` takes keyword arguments now gets `perform_later` and `perform_now`, and its keywords travel in the payload the way ActiveJob writes them.

Until now `job_class_side` skipped any `perform` with a keyword parameter, leaving only a residue warning, so the class got no entry points. The `perform_later(widget_id: ...)` call stayed in place: a `NoMethodError` on ruby, and a refused call on spinel.

## What changes

- **The wrappers take `perform`'s own parameter list** (`def self.perform_later(article, count:, label: "touched")`) and forward each keyword by name (`new.perform(article, count: count, label: label)`). A `**rest` keyword, a block, or `...` forwarding still gets no wrapper, as before. Native Sidekiq workers keep their positional-only rule, since Sidekiq passes their args positionally.
- **`lower::job_payload` plans keywords from the call sites**, as it plans positional arguments. They are written as one trailing Hash marked `_aj_ruby2_keywords`, which is what activejob 8.1.3 writes for `perform_later(room, count: 2)`. The decoder passes them back to `perform` by name.
- **An optional keyword whose value equals its literal default is left out**, as Rails leaves out a keyword the caller omits, and an absent key reads back as that default. A non-literal default, a `**` splat at a call site, or a rest parameter beside keywords keeps the job's Proc. That fallback is an error under `:sidekiq`, as it already is for positional arguments.
- **The runtime gains two helpers**: `ActiveJob::Arguments.keywords(names, values)` writes the Hash, and `keywords_at(args, i, names)` reads it. Anything that isn't a keyword Hash is a `DeserializationError`.
- `runtime.md` describes the format and its two known differences from Rails:
  - The keys are written in `perform`'s order, not the call site's.
  - A caller that passes the default explicitly gets no key where Rails writes one.
  - `perform` receives the same values either way.

## How it's tested

- **New: `tests/emit_and_run/job_keywords.rs`.** It covers a positional record plus keywords and a namespaced keyword-only job. Each is enqueued with every keyword and with an optional one omitted, then run through the drain, by `perform_now`, and with a record deleted before the drain (discarded). It fails on `main` with no payload writer or `perform_later` for the job. A second test emits ruby and spinel trees on `queue_adapter :sidekiq` with zero errors and checks the generated push and decode.
- **`tests/active_job_payload`:** the oracle now also records activejob 8.1.3's `serialize` for three keyword calls (record plus keywords, an omitted optional keyword, and time, symbol, array and STI record values). The driver's output matches it byte for byte, key order included, and reads each value back.
- **`tests/sidekiq_push`:** the oracle now pushes two keyword jobs through Sidekiq 8.1.7's ActiveJob adapter against a real Redis. The driver's envelopes match them. The eight existing pushes in `sidekiq.json` are unchanged.
- The `job`, `sidekiq` and `perform` emit_and_run tests, `active_job_payload`, `sidekiq_push`, `active_job_enqueue_log` and `active_job_cruby_drain` pass.

🤖 Generated with [Claude Code](https://claude.com/claude-code)

//! Native Sidekiq jobs run by Roundhouse's worker (`JobRegistry.perform_native`,
//! `SidekiqWorker.run_native`): a job the app pushed is read back with its
//! worker's codecs and `perform` is called with as many arguments as the
//! job carries, as Sidekiq 8.1.7's processor does. The `args` scenario of
//! `tests/sidekiq_native/sidekiq_run.json` is what real Sidekiq ran.
//! The Spinel binary runs the same readers in `tests/sidekiq_worker.rs`'s
//! worker driver: this harness doesn't build a Spinel tree on `:sidekiq`,
//! which needs spinel-redis.

use super::emit_and_run;
use roundhouse::project::BuildTarget;

const SIDEKIQ: &str = "  config.active_job.queue_adapter = :sidekiq\n";
const REDIS_CLIENT: &str = "Sidekiq.configure_client do |config|\n  config.redis = { url: ENV.fetch(\"REDIS_URL\", \"redis://localhost:6379/0\") }\nend\n";

/// The oracle's `ArgsWorker`, with a Hash of Strings where the oracle's
/// mixes an Array in, which a typed `perform` can't hold.
const ARGS_WORKER: &str = "class ArgsWorker\n  include Sidekiq::Job\n  NOTES = [\"\"]\n\n  def perform(id, name, opts = {}, list = [], flag = nil)\n    NOTES << JSON.generate([id, name, opts, list, flag])\n    nil\n  end\nend\n";

fn on_sidekiq(worker: &str, calls: &str) -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/workers/args_worker.rb", worker)
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            &format!("  validates :title, presence: true\n\n  def self.enqueue_native\n{calls}  end\n"),
        )
        .edit("config/environments/production.rb", "  config.active_job.queue_adapter = :solid_queue\n", SIDEKIQ)
        .write("config/initializers/sidekiq.rb", REDIS_CLIENT)
}

const CALLS: &str = "    ArgsWorker.perform_async(1, \"n\")\n    ArgsWorker.perform_async(2, \"n\", { \"k\" => \"v\" }, [1, 2], true)\n";

const SCRIPT: &str = r#"ActiveJob.register_drain
SidekiqPush.start_recording
Article.enqueue_native
raws = SidekiqPush.recorded.select { |l| l.start_with?("lpush queue:default ") }.map { |l| l.sub("lpush queue:default ", "") }
SidekiqPush.stop_recording
raise "pushed #{raws.length}" unless raws.length == 2
ArgsWorker::NOTES.clear
raws.each { |raw| SidekiqWorker.run("default", raw) }
want = ["[1,\"n\",{},[],null]", "[2,\"n\",{\"k\":\"v\"},[1,2],true]"]
raise "ran #{ArgsWorker::NOTES.inspect}" unless ArgsWorker::NOTES == want

def refused(args)
  JobRegistry.perform_native("ArgsWorker", args, "0123456789abcdef01234567")
  "ran"
rescue StandardError => e
  e.class.name + ": " + e.message
end
got = refused(["1", "n"])
raise got unless got == "TypeError: ArgsWorker#perform argument 0 (`id`): expected an Integer, got a String"
got = refused([1, "n", { "k" => 1 }])
raise got unless got == "TypeError: ArgsWorker#perform argument 2 (`opts`): expected a String, got an Integer"
got = refused([1])
raise got unless got == "ArgumentError: wrong number of arguments (given 1, expected 2..5)"
got = refused([1, "n", {}, [], nil, 6])
raise got unless got == "ArgumentError: wrong number of arguments (given 6, expected 2..5)"
got = refused({ "id" => 1 })
raise got unless got == "TypeError: a native job's args must be a JSON array"
raise "an unknown class ran" if JobRegistry.perform_native("Kernel", [], "")
raise "a refused job ran" unless ArgsWorker::NOTES.length == 2
puts "native jobs ran as Sidekiq runs them"
"#;

#[test]
fn native_jobs_run_as_sidekiq_runs_them() {
    let run = on_sidekiq(ARGS_WORKER, CALLS).run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("native jobs ran as Sidekiq runs them"), "{}", run.stdout);
}

/// A nil inside an Array has no typed slot to read back into, so pushing
/// such a job is a compile error rather than a job that fails in the worker.
#[test]
fn an_argument_the_worker_cant_read_back_is_an_error() {
    let worker = "class ArgsWorker\n  include Sidekiq::Job\n\n  def perform(list)\n    nil\n  end\nend\n";
    let (_, errors) = on_sidekiq(worker, "    ArgsWorker.perform_async([1, nil])\n").emit(BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("can't read back")), "{errors:#?}");
}

/// `tests/sidekiq_native/oracle_jobs.rb`'s retry-hook workers, as app code.
const HOOK_WORKERS: &[(&str, &str)] = &[
    ("app/models/sticky_error.rb", "class StickyError < StandardError\nend\n"),
    ("app/models/run_notes.rb", "module RunNotes\n  NOTES = [\"\"]\n\n  def self.note(name, value)\n    NOTES << name + \" \" + value\n    nil\n  end\nend\n"),
    ("app/workers/retry_in_worker.rb", "class RetryInWorker\n  include Sidekiq::Job\n  sidekiq_options retry: 2\n  sidekiq_retry_in do |count|\n    (count**4) + 15 + (count * 10)\n  end\n\n  def perform(id)\n    RunNotes.note(\"retry_in\", id.to_s)\n    raise StickyError, \"retry_in #{id}\"\n  end\nend\n"),
    ("app/workers/retry_in_kill_worker.rb", "class RetryInKillWorker\n  include Sidekiq::Job\n  sidekiq_options retry: 5\n  sidekiq_retry_in do |count, exception, msg|\n    RunNotes.note(\"retry_in_kill_block\", \"#{count} #{exception.class} #{msg[\"args\"].inspect}\")\n    :kill\n  end\n\n  def perform(id)\n    raise StickyError, \"kill #{id}\"\n  end\nend\n"),
    ("app/workers/retry_in_discard_worker.rb", "class RetryInDiscardWorker\n  include Sidekiq::Job\n  sidekiq_options retry: 5\n  sidekiq_retry_in { |count| :discard }\n\n  def perform(id)\n    raise StickyError, \"discard #{id}\"\n  end\nend\n"),
    ("app/workers/retry_in_nil_worker.rb", "class RetryInNilWorker\n  include Sidekiq::Job\n  sidekiq_options retry: 1\n  sidekiq_retry_in { |count| nil }\n\n  def perform(id)\n    raise StickyError, \"nil #{id}\"\n  end\nend\n"),
    ("app/workers/exhausted_worker.rb", "class ExhaustedWorker\n  include Sidekiq::Job\n  sidekiq_options retry: 1\n  sidekiq_retries_exhausted do |msg, exception|\n    RunNotes.note(\"exhausted\", \"#{msg[\"args\"].first} #{exception.class}: #{exception.message} \" \\\n      \"retry_count=#{msg[\"retry_count\"]} keys=#{msg.keys.join(\",\")}\")\n  end\n\n  def perform(id)\n    raise StickyError, \"exhausted #{id}\"\n  end\nend\n"),
    ("app/workers/exhausted_not_dead_worker.rb", "class ExhaustedNotDeadWorker\n  include Sidekiq::Job\n  sidekiq_options retry: 0, dead: false\n  sidekiq_retries_exhausted do |msg, _ex|\n    RunNotes.note(\"exhausted_not_dead\", msg[\"args\"].first.to_s)\n  end\n\n  def perform(id)\n    raise StickyError, \"not dead #{id}\"\n  end\nend\n"),
];

/// The oracle's scenarios: its name, the worker, and the id it pushes.
const HOOK_SCENARIOS: &[(&str, &str, i64)] = &[
    ("retry_in", "RetryInWorker", 1),
    ("retry_in_kill", "RetryInKillWorker", 1),
    ("retry_in_discard", "RetryInDiscardWorker", 1),
    ("retry_in_nil", "RetryInNilWorker", 1),
    ("exhausted", "ExhaustedWorker", 42),
    ("exhausted_not_dead", "ExhaustedNotDeadWorker", 43),
];

/// Each scenario's job runs, and each retry record it writes runs again,
/// as Sidekiq's poller would put it back, until it writes none. Prints
/// `ran`, and one `write <set> <seconds from now> <keys> <record>` line
/// per write, per scenario.
fn hook_script() -> String {
    let mut s = String::from(
        "WRITES = [[\"\", \"\"]]\nWRITES.clear\nmodule SidekiqWorker\n  def self.write_retry(at, json)\n    WRITES << [\"retry \" + (at - Time.now.to_f).to_s, json]\n    nil\n  end\n\n  def self.write_dead(json)\n    WRITES << [\"dead 0\", json]\n    nil\n  end\nend\nActiveJob.register_drain\n",
    );
    for (name, _, _) in HOOK_SCENARIOS {
        s.push_str(&format!(
            "RunNotes::NOTES.clear\nWRITES.clear\nSidekiqPush.start_recording\nArticle.hook_{name}\nraw = SidekiqPush.recorded.find {{ |l| l.start_with?(\"lpush queue:default \") }}.sub(\"lpush queue:default \", \"\")\nSidekiqPush.stop_recording\n\
             runs = 0\nwhile !raw.nil? && runs < 5\n  SidekiqWorker.run(\"default\", raw)\n  runs += 1\n  last = WRITES.last\n  raw = !last.nil? && last[0].start_with?(\"retry \") && WRITES.length == runs ? last[1] : nil\nend\n\
             puts \"scenario {name}\"\nRunNotes::NOTES.each {{ |n| puts \"ran \" + n }}\nWRITES.each {{ |w| puts \"write \" + w[0] + \" \" + JSON.parse(w[1]).keys.join(\",\") + \" \" + w[1] }}\n"
        ));
    }
    s.push_str("puts \"hooks done\"\n");
    s
}

/// What real Sidekiq 8.1.7 ran and wrote for each scenario
/// (`tests/sidekiq_native/sidekiq_run.json`): the same notes, and the same
/// writes, set by set, with the same keys in the same order. A retry is
/// due when Sidekiq scored it: the block's delay (or the default
/// `count**4 + 15`), plus `rand(10 * (count + 1))`.
#[test]
fn retry_hooks_run_as_sidekiq_runs_them() {
    let mut overlay = on_sidekiq(ARGS_WORKER, CALLS);
    for (path, source) in HOOK_WORKERS {
        overlay = overlay.write(path, source);
    }
    let sites: String =
        HOOK_SCENARIOS.iter().map(|(name, worker, id)| format!("\n  def self.hook_{name}\n    {worker}.perform_async({id})\n  end\n")).collect();
    overlay = overlay.edit("app/models/article.rb", "  validates :title, presence: true\n", &format!("  validates :title, presence: true\n{sites}"));
    let run = overlay.run_ruby(&hook_script());
    run.assert_passes();
    assert!(run.stdout.contains("hooks done"), "{}", run.stdout);
    let oracle: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string("tests/sidekiq_native/sidekiq_run.json").unwrap()).unwrap();
    let mut got: std::collections::BTreeMap<String, (Vec<String>, Vec<(String, f64, String, i64)>)> = Default::default();
    let mut current = String::new();
    for line in run.stdout.lines() {
        if let Some(name) = line.strip_prefix("scenario ") {
            current = name.to_string();
            got.entry(current.clone()).or_default();
        } else if let Some(note) = line.strip_prefix("ran ") {
            got.get_mut(&current).unwrap().0.push(note.to_string());
        } else if let Some(rest) = line.strip_prefix("write ") {
            let mut parts = rest.splitn(4, ' ');
            let (set, secs, keys, record) = (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap());
            let count = serde_json::from_str::<serde_json::Value>(record).unwrap()["retry_count"].as_i64().unwrap();
            got.get_mut(&current).unwrap().1.push((set.to_string(), secs.parse().unwrap(), keys.to_string(), count));
        }
    }
    for scenario in oracle["scenarios"].as_array().unwrap().iter().filter(|s| s["name"] != "args") {
        let name = scenario["name"].as_str().unwrap();
        let (ran, writes) = got.get(name).unwrap_or_else(|| panic!("{name} did not run: {}", run.stdout));
        // The oracle's `retry_in` note carries the jid, which this app's worker can't read yet.
        let want_ran: Vec<String> = scenario["ran"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap().split(" jid=").next().unwrap().to_string())
            .collect();
        assert_eq!(ran, &want_ran, "{name}");
        let want: Vec<(String, String, i64)> = scenario["writes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| {
                let keys: Vec<&str> = w["keys"].as_array().unwrap().iter().map(|k| k.as_str().unwrap()).collect();
                (w["set"].as_str().unwrap().to_string(), keys.join(","), w["record"]["retry_count"].as_i64().unwrap())
            })
            .collect();
        let have: Vec<(String, String, i64)> = writes.iter().map(|(s, _, k, c)| (s.clone(), k.clone(), *c)).collect();
        assert_eq!(have, want, "{name}");
        for (set, secs, _, count) in writes {
            if set != "retry" {
                continue;
            }
            let base = if name == "retry_in" { (count.pow(4) + 15 + count * 10) as f64 } else { (count.pow(4) + 15) as f64 };
            let jitter = (10 * (count + 1)) as f64;
            assert!(*secs > base - 1.0 && *secs < base + jitter + 1.0, "{name} retry {count} due in {secs}s, want {base} + rand({jitter})");
        }
    }
}

/// A retry block Roundhouse can't call as Sidekiq calls it is an error,
/// not a worker whose failed jobs quietly take the default schedule.
#[test]
fn a_retry_block_that_cant_be_called_is_an_error() {
    let worker = "class ArgsWorker\n  include Sidekiq::Job\n  sidekiq_retry_in do |*counts|\n    10\n  end\n\n  def perform(id, name, opts = {}, list = [], flag = nil)\n    nil\n  end\nend\n";
    let (_, errors) = on_sidekiq(worker, CALLS).emit(BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("takes no block Roundhouse can call")), "{errors:#?}");
}

/// A `perform` that reads `jid` reads the job's, as Sidekiq sets it.
#[test]
fn a_native_job_reads_its_jid() {
    let worker = "class ArgsWorker\n  include Sidekiq::Job\n  NOTES = [\"\"]\n\n  def perform(id, name, opts = {}, list = [], flag = nil)\n    NOTES << jid\n    nil\n  end\nend\n";
    let script = "NOTES = ArgsWorker::NOTES\nNOTES.clear\nJobRegistry.perform_native(\"ArgsWorker\", [1, \"n\"], \"0123456789abcdef01234567\")\nraise NOTES.inspect unless NOTES == [\"0123456789abcdef01234567\"]\nputs \"jid read\"\n";
    let run = on_sidekiq(worker, CALLS).run_ruby(script);
    run.assert_passes();
    assert!(run.stdout.contains("jid read"), "{}", run.stdout);
}

/// An ActiveJob job's Sidekiq retry block is an error: Roundhouse's worker
/// doesn't read it through the wrapper as Sidekiq does.
#[test]
fn an_active_job_retry_block_is_an_error() {
    let job = "class HookJob < ApplicationJob\n  sidekiq_retry_in { |count| 10 }\n\n  def perform(id)\n    nil\n  end\nend\n";
    let (_, errors) = on_sidekiq(ARGS_WORKER, CALLS)
        .write("app/jobs/hook_job.rb", job)
        .edit("app/models/article.rb", "  validates :title, presence: true\n", "  validates :title, presence: true\n\n  def self.hook_job\n    HookJob.perform_later(1)\n  end\n")
        .emit(BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("isn't called by Roundhouse's Sidekiq worker")), "{errors:#?}");
}

/// A hook works on the message that is written, as `JobRetry` passes its
/// `msg`: a `retry_in` that moves the job to another queue moves the retry.
#[test]
fn a_hooks_change_to_the_message_is_written() {
    let worker = "class ArgsWorker\n  include Sidekiq::Job\n  sidekiq_options retry: 3\n  sidekiq_retry_in do |count, exception, msg|\n    msg[\"queue\"] = \"low\"\n    30\n  end\n\n  def perform(id, name, opts = {}, list = [], flag = nil)\n    raise ArgumentError, \"no\"\n  end\nend\n";
    let script = "WRITES = [\"\"]\nWRITES.clear\nmodule SidekiqWorker\n  def self.write_retry(at, json)\n    WRITES << json\n    nil\n  end\nend\nActiveJob.register_drain\nSidekiqPush.start_recording\nArticle.enqueue_native\nraw = SidekiqPush.recorded.find { |l| l.start_with?(\"lpush queue:default \") }.sub(\"lpush queue:default \", \"\")\nSidekiqPush.stop_recording\nSidekiqWorker.run(\"default\", raw)\nraise WRITES.inspect unless WRITES.length == 1 && JSON.parse(WRITES[0])[\"queue\"] == \"low\"\nputs \"hook change written\"\n";
    let run = on_sidekiq(worker, CALLS).run_ruby(script);
    run.assert_passes();
    assert!(run.stdout.contains("hook change written"), "{}", run.stdout);
}

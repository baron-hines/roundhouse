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

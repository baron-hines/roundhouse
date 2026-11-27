# The worker role as its own process, as `./app sidekiq ...` runs it:
# tests/sidekiq_worker_live.rs starts it, pushes jobs through real
# Rails (rails_push.rb), and signals it from outside with TSTP and TERM.
# Each job prints one line the harness waits on. SlowJob sleeps
# SLOW_SECONDS (5 by default), past the `-t 1` a first worker is given,
# so TERM finds it running and pushes it back; a second worker, given a
# short SLOW_SECONDS, then finishes it.
require_relative "stubs"

module JobRegistry
  def self.perform(job_class, args)
    label = ActiveJob::Arguments.str_at(args, 0)
    if job_class == "RecordJob"
      say("ran " + label)
      true
    elsif job_class == "SlowJob"
      say("slow started " + label)
      sleep slow_seconds
      say("slow done " + label)
      true
    else
      false
    end
  end

  def self.discards_deserialization_error(job_class)
    false
  end

  def self.slow_seconds
    given = ENV["SLOW_SECONDS"].to_s
    given.length > 0 ? given.to_f : 5.0
  end

  def self.say(line)
    $stdout.print line + "\n"
    $stdout.flush
    nil
  end
end

SidekiqWorker.cli(ARGV)

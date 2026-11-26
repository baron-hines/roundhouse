# The worker role against a real Redis: the runtime a tree on
# queue_adapter :sidekiq runs as `./app sidekiq` (runtime/spinel/sidekiq/
# sidekiq_worker.rb over runtime/spinel/sidekiq/sidekiq_redis.rb), with
# redis-rb under CRuby and spinel-redis when spinel compiles this file.
#
# tests/sidekiq_worker_live.rs copies it beside the runtime sources and
# runs it with REDIS_URL naming a scratch database. It pushes jobs the
# way the web role does, runs two processor threads until they have
# taken them all, then shuts down with a slow job still running, as
# TERM does. On the way, the scheduler moves a job pushed for a second
# later off the schedule set, and a dead-set write goes through the
# client's MULTI. reader.rb then reads what is left through Sidekiq's
# own API: the retry set, the dead set, and the slow job back on its
# queue.
require_relative "stubs"

module JobRegistry
  RAN = [""]
  RAN_LOCK = Mutex.new

  def self.perform(job_class, args)
    if job_class == "BoomJob"
      raise ArgumentError, "boom " + ActiveJob::Arguments.str_at(args, 0)
    elsif job_class == "SlowJob"
      RAN_LOCK.synchronize { RAN.push("slow started") }
      sleep 5
      true
    elsif job_class == "RecordJob"
      RAN_LOCK.synchronize { RAN.push(ActiveJob::Arguments.str_at(args, 0)) }
      true
    else
      false
    end
  end

  def self.discards_deserialization_error(job_class)
    false
  end
end
JobRegistry::RAN.clear

def push(job_class, queue, label)
  push_at(job_class, queue, label, 0.0)
end

def push_at(job_class, queue, label, at)
  SidekiqPush.enqueue(job_class, queue, at, ActiveJob::Payload.build_at(job_class, queue, at,
    ActiveJob::Arguments.list([ActiveJob::Arguments.str(label)])))
end

def ran
  JobRegistry::RAN_LOCK.synchronize { JobRegistry::RAN.dup }
end

def wait_until(what, seconds)
  waited = 0.0
  while !yield
    raise "timed out waiting for " + what if waited > seconds
    sleep 0.05
    waited += 0.05
  end
  nil
end

push("RecordJob", "default", "r1")
push("RecordJob", "low", "r2")
push("BoomJob", "default", "b1")
push("MailerJob", "low", "m1")
push("RecordJob", "default", "r3")

probe = SidekiqRedis.worker_open(SidekiqPush.url)
SidekiqWorker.parse(["sidekiq", "-q", "default", "-q", "low", "-c", "2", "-t", "1"])
SidekiqWorker.install_traps
SidekiqWorker.start(2)
wait_until("the first jobs", 20.0) { ran.length == 3 && probe.zcard("retry") == 2 }
puts "ran " + ran.sort.join(",")

# The scheduler moves a job pushed for a second from now off the
# schedule set (its Lua pop, then the push), and a processor runs it.
# The two failed jobs stay in the retry set: their backoff is 15 s.
SidekiqScheduler.start(0.5, -1.0)
push_at("RecordJob", "default", "later", Time.now.to_f + 1.0)
wait_until("the scheduled job", 20.0) { ran.include?("later") }
puts "scheduled ran, " + probe.zcard("schedule").to_s + " left, " + probe.zcard("retry").to_s + " to retry"

# The dead-set write a job out of retries takes: ZADD and Sidekiq's two
# trims in one MULTI. An entry older than :dead_timeout_in_seconds: is
# trimmed by age as it goes.
now = Time.now.to_f
probe.zadd("dead", now - 20000000.0, "{\"jid\":\"ancient\"}")
SidekiqRedis.write_dead(probe, now,
  "{\"retry\":0,\"queue\":\"default\",\"class\":\"Sidekiq::ActiveJob::Wrapper\",\"wrapped\":\"DeadJob\",\"args\":[],\"jid\":\"dddddddddddddddddddddddd\",\"error_class\":\"ArgumentError\",\"error_message\":\"out of retries\",\"failed_at\":" + now.to_s + ",\"retry_count\":0}",
  now - 15552000.0, "-10000")
puts "dead " + probe.zcard("dead").to_s

push("SlowJob", "default", "s1")
wait_until("the slow job", 20.0) { ran.include?("slow started") }
Process.kill("TERM", Process.pid)
SidekiqWorker.supervise
puts "requeued, " + probe.llen("queue:default").to_s + " on default"
probe.close
puts "worked " + SidekiqPush.redacted_url(SidekiqPush.url)
SidekiqRedis.hard_exit(0)

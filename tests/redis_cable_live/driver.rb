# The Redis broadcast transport (runtime/spinel/sidekiq/redis_cable.rb)
# against a real Redis, as a web process runs it: under CRuby over
# redis-rb, and over spinel-redis when spinel compiles this file.
#
# tests/redis_cable_live.rs copies it beside the runtime sources and runs
# it with REDIS_URL naming a scratch database. It subscribes as the first
# socket does, prints `subscribed`, and prints `got <stream> <payload>`
# for each message, which `Cable.deliver_json` (here a recorder) is
# handed. After the three rails_broadcast.rb sends, it publishes each one
# back on `echo_<stream>` through RedisCable.publish, for that script's
# listener to compare with what Rails sent.
require "json"

module SidekiqPush
  def self.password
    ""
  end
end

require_relative "sidekiq_redis"

module JobRegistry
  def self.cable_redis?
    true
  end

  def self.cable_redis_url
    ENV["REDIS_URL"].to_s
  end

  def self.cable_channel_prefix
    "depot_production"
  end
end

module Cable
  GOT_STREAMS = [""]
  GOT_PAYLOADS = [""]
  GOT_LOCK = Mutex.new

  def self.deliver_json(stream, message_json)
    GOT_LOCK.synchronize do
      GOT_STREAMS.push(stream)
      GOT_PAYLOADS.push(message_json)
    end
    $stdout.print "got " + stream + " " + message_json + "\n"
    $stdout.flush
    nil
  end

  def self.count
    GOT_LOCK.synchronize { GOT_STREAMS.length }
  end
end
Cable::GOT_STREAMS.clear
Cable::GOT_PAYLOADS.clear

require_relative "redis_cable"

RedisCable.ensure_subscriber
raise "no subscription within " + RedisCable::SUBSCRIBE_WAIT.to_s + " s" unless RedisCable::SUBSCRIBED[0]
$stdout.print "subscribed\n"
$stdout.flush

waited = 0.0
while Cable.count < 3
  raise "timed out waiting for Rails' broadcasts" if waited > 30.0
  sleep 0.05
  waited += 0.05
end
i = 0
while i < 3
  stream = Cable::GOT_LOCK.synchronize { Cable::GOT_STREAMS[i] }
  payload = Cable::GOT_LOCK.synchronize { Cable::GOT_PAYLOADS[i] }
  RedisCable.publish("echo_" + stream, payload)
  i += 1
end
$stdout.print "echoed\n"
$stdout.flush
SidekiqRedis.hard_exit(0)

# What Rails publishes to Redis for a broadcast, recorded from
# actioncable 8.1.3's own Redis adapter (`adapter: redis` with a
# `channel_prefix`, as config/cable.yml sets it) against a private
# redis-server. A pattern subscriber records each PUBLISH's channel and
# payload bytes for three broadcasts: a Turbo Stream fragment (the
# String turbo-rails hands `ActionCable.server.broadcast`), a Hash, and
# an already-encoded String with `coder: nil`, as Campfire sends its
# unread notices. Writes rails.json beside this file:
#
#   ruby tests/redis_cable/oracle.rb
#
# tests/redis_cable.rs holds the runtime's publishes to these bytes.
gem "actioncable", "8.1.3"
gem "activesupport", "8.1.3"
require "active_support"
require "active_support/core_ext"
require "action_cable"
require "redis"
require "json"

PORT = 16540
system("redis-server", "--port", PORT.to_s, "--save", "", "--appendonly", "no",
  "--daemonize", "yes", "--pidfile", "/tmp/redis-cable-oracle.pid",
  "--logfile", "/tmp/redis-cable-oracle.log", exception: true)
50.times do
  break if system("redis-cli", "-p", PORT.to_s, "ping", out: File::NULL, err: File::NULL)
  sleep 0.1
end
at_exit { system("redis-cli", "-p", PORT.to_s, "shutdown", "nosave", out: File::NULL, err: File::NULL) }

URL = "redis://127.0.0.1:#{PORT}/1"
seen = Queue.new
ready = Queue.new
listener = Thread.new do
  Redis.new(url: URL).psubscribe("*") do |on|
    on.psubscribe { |_pattern, _count| ready << true }
    on.pmessage { |_pattern, channel, message| seen << [channel, message] }
  end
end
ready.pop

server = ActionCable::Server::Base.new
server.config.cable = { adapter: "redis", url: URL, channel_prefix: "depot_production" }
server.config.logger = Logger.new(nil)

fragment = %(<turbo-stream action="append" target="cars"><template><div id="car_1">Golf &amp; "GTI" é</div></template></turbo-stream>)
broadcasts = [
  ["turbo stream fragment", "Z2lkOi8vZGVwb3QvQ2FyLzE", fragment, :json],
  ["hash", "user_7_unreads", { "roomId" => 1, "count" => 2, "note" => "<b>&</b>" }, :json],
  ["coder: nil", "user_7_unreads", JSON.generate({ "roomId" => 3 }), nil],
]
out = []
broadcasts.each do |name, stream, message, coder|
  if coder.nil?
    server.broadcast(stream, message, coder: nil)
  else
    server.broadcast(stream, message)
  end
  channel, payload = seen.pop
  out << { "name" => name, "stream" => stream, "channel" => channel, "payload" => payload }
end
listener.kill
listener.join rescue nil

File.write(File.join(__dir__, "rails.json"), JSON.pretty_generate({
  "actioncable" => Gem.loaded_specs["actioncable"].version.to_s,
  "channel_prefix" => "depot_production",
  "publishes" => out,
}) + "\n")
puts "recorded #{out.length}"

# Rails' side of tests/redis_cable_live.rs: actioncable 8.1.3's Redis
# adapter on REDIS_URL with the driver's channel prefix. It broadcasts
# tests/redis_cable/rails.json's three messages, then waits for the
# driver to publish each back on `echo_<stream>` and checks the channel
# and the bytes, one ok/FAIL line each.
gem "actioncable", "8.1.3"
gem "activesupport", "8.1.3"
require "active_support"
require "active_support/core_ext"
require "action_cable"
require "redis"
require "json"

url = ENV.fetch("REDIS_URL")
oracle = JSON.parse(File.read(File.join(__dir__, "rails.json")))
prefix = oracle["channel_prefix"]

echoes = Queue.new
ready = Queue.new
listener = Thread.new do
  Redis.new(url: url).psubscribe("#{prefix}:echo_*") do |on|
    on.psubscribe { |_pattern, _count| ready << true }
    on.pmessage { |_pattern, channel, message| echoes << [channel, message] }
  end
end
ready.pop

server = ActionCable::Server::Base.new
server.config.cable = { adapter: "redis", url: url, channel_prefix: prefix }
server.config.logger = Logger.new(nil)

fragment = %(<turbo-stream action="append" target="cars"><template><div id="car_1">Golf &amp; "GTI" é</div></template></turbo-stream>)
server.broadcast("Z2lkOi8vZGVwb3QvQ2FyLzE", fragment)
server.broadcast("user_7_unreads", { "roomId" => 1, "count" => 2, "note" => "<b>&</b>" })
server.broadcast("user_7_unreads", JSON.generate({ "roomId" => 3 }), coder: nil)

def check(name, ok, detail)
  puts (ok ? "ok   " : "FAIL ") + name + (ok ? "" : " (#{detail})")
end

oracle["publishes"].each do |want|
  channel, payload = Timeout.timeout(30) { echoes.pop }
  check("#{want["name"]}: echo channel", channel == "#{prefix}:echo_#{want["stream"]}", channel)
  check("#{want["name"]}: echo bytes", payload == want["payload"], payload)
end
listener.kill
puts "done"

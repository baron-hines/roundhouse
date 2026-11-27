# The oracle for tests/remote_ip_class.rs: an app that reopens
# `class ActionDispatch::RemoteIp`, on actionpack 8.1.4.
#
#   ruby tests/remote_ip_class/oracle.rb
#
# Needs actionpack 8.1.4. RemoteIp is the middleware class, so the
# reopening loads, and IpSpoofAttackError keeps its path. It prints
# "remote ip passed".
gem "actionpack", "8.1.4"
require "action_dispatch"
%w[remote_ip script].each { |f| load File.join(__dir__, "#{f}.rb") }

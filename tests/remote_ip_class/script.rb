def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
check("RemoteIp is a class", ActionDispatch::RemoteIp.is_a?(Class), true)
check("the app's reopening is loaded", ActionDispatch::RemoteIp::ProxyCount.proxies, 2)
check("IpSpoofAttackError's path", ActionDispatch::RemoteIp::IpSpoofAttackError.name, "ActionDispatch::RemoteIp::IpSpoofAttackError")
check("IpSpoofAttackError is a StandardError", ActionDispatch::RemoteIp::IpSpoofAttackError.superclass, StandardError)
puts "remote ip passed"

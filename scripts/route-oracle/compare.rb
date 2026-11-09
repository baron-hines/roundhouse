# Lines up Rails' route table (`rails_routes.rb`) with roundhouse's
# (`dump_routes`) and prints where they differ:
#   ruby compare.rb rails.txt roundhouse.txt
#
# A route is VERB, PATH (segment names ignored: `:id` and `:post_id` are
# one shape) and controller#action. Order matters only among routes of
# the same verb and shape, where the first wins. Rails names a route
# once, on its first line; names are compared as name -> path.

require "uri"

Route = Struct.new(:verb, :path, :endpoint, :name, :state) do
  def shape = path.gsub(/:[a-z_0-9]+/, ":").gsub(/\*[a-z_0-9]+/, "*")
  def key = [verb, shape]
end

def read(file)
  File.readlines(file, chomp: true).filter_map do |line|
    verb, path, endpoint, name, state = line.split(" ")
    # Rails percent-encodes a non-ASCII literal segment; roundhouse keeps the characters.
    Route.new(verb, URI.decode_www_form_component(path), endpoint, name, state || "dispatched") if verb
  end
end

rails = read(ARGV.fetch(0)).reject { |r| %w[INTERNAL ENGINE RACK].include?(r.endpoint) }
# Rails' own engines' routes (`/rails/active_storage`, `/rails/action_mailbox`, `/rails/conductor`):
# roundhouse serves Active Storage from its runtime table, not the app's, so these are counted apart.
framework, rails = rails.partition { |r| r.path.start_with?("/rails/") }
ours = read(ARGV.fetch(1))
# A route roundhouse knows it cannot run (`RoundhouseUnmodeledEndpoint`) is reported, not served.
unmodeled, ours = ours.partition { |r| r.endpoint.start_with?("RoundhouseUnmodeledEndpoint") }
served = ours.reject { |r| r.state.start_with?("refused") }

first = ->(routes) { routes.each_with_object({}) { |r, h| h[r.key] ||= r } }
rails_first, ours_first = first.(rails), first.(served)

missing = rails_first.reject { |k, _| ours_first.key?(k) || unmodeled.any? { |u| u.key == k } }.values
extra = ours_first.reject { |k, _| rails_first.key?(k) }.values
# Where roundhouse reports its own first route as unmodeled, the next one it serves is no finding.
wrong = rails_first.filter_map { |k, r| (o = ours_first[k]) && o.endpoint != r.endpoint && unmodeled.none? { |u| u.key == k } ? [r, o] : nil }
refused = ours.select { |r| r.state.start_with?("refused") }.group_by(&:state).transform_values(&:size)

# A name covers its route and the unnamed expansions after it (`/web(/*any)` is two lines, named once,
# and the two sides may list the expansions in either order).
def named_shapes(routes)
  out = {}
  current = nil
  routes.each do |r|
    if r.name != "-"
      current = out.key?(r.name) ? nil : (out[r.name] = [r.shape]; r)
    elsif current && r.endpoint == current.endpoint && r.verb == current.verb
      out[current.name] << r.shape
    else
      current = nil
    end
  end
  out
end
rails_named, our_named = named_shapes(rails), named_shapes(ours)
rails_names = rails_named.transform_values(&:first)
our_names = our_named.transform_values(&:first)
names_missing = rails_names.keys - our_names.keys
# Each side names one expansion of a route, not always the same one: a name is at another path only
# when no route of its verb and endpoint on our side has the shape Rails names.
by_endpoint = ours.group_by { |r| [r.verb, r.endpoint] }.transform_values { |rs| rs.map(&:shape) }
named_routes = ours.reject { |r| r.name == "-" }.each_with_object({}) { |r, h| h[r.name] ||= r }
names_moved = rails_names.select do |n, shape|
  ours_route = named_routes[n]
  ours_route && !by_endpoint.fetch([ours_route.verb, ours_route.endpoint], []).include?(shape) && !(our_named[n] || []).include?(shape)
end.keys

puts "rails #{rails_first.size} routes, roundhouse #{ours_first.size} served (#{refused.values.sum} refused: #{refused.map { |k, v| "#{k.sub("refused:", "")} #{v}" }.join(", ")})"
puts "framework routes (/rails/*) #{framework.size}, known unmodeled endpoints #{unmodeled.size}, guarded #{served.count { |r| r.state == "guarded" }}"
puts "missing #{missing.size}, extra #{extra.size}, different endpoint #{wrong.size}, helpers missing #{names_missing.size}, helpers at another path #{names_moved.size}"
puts "\n## missing (Rails serves, roundhouse does not)"; missing.each { |r| puts "  #{r.verb} #{r.path} #{r.endpoint}#{ours.any? { |o| o.key == r.key } ? "  [refused here]" : ""}" }
puts "\n## extra (roundhouse serves, Rails does not)"; extra.each { |r| puts "  #{r.verb} #{r.path} #{r.endpoint}" }
puts "\n## different endpoint (rails -> roundhouse)"; wrong.each { |r, o| puts "  #{r.verb} #{r.path} #{r.endpoint} -> #{o.endpoint}" }
puts "\n## helpers missing"; names_missing.each { |n| puts "  #{n} #{rails_names[n]}" }
puts "\n## helpers at another path"; names_moved.each { |n| puts "  #{n} rails #{rails_names[n]} roundhouse #{our_names[n]}" }

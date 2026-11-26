# Rails' own route table, one route per line in `dump_routes`' shape:
#   VERB PATH controller#action NAME
# Run inside the app: `ORACLE_OUT=rails.txt bin/rails runner path/to/rails_routes.rb`
# (a file, not stdout, which the app's own boot logging shares).
# `(.:format)` is dropped and every other optional group is expanded, as
# roundhouse flattens them. Engine mounts and Rails' own internal routes
# are printed with `ENGINE`/`INTERNAL` in place of the endpoint.

def expansions(spec)
  open = spec.index("(")
  return [spec] if open.nil?
  depth = 0
  close = (open...spec.length).find do |i|
    depth += 1 if spec[i] == "("
    depth -= 1 if spec[i] == ")"
    depth.zero?
  end
  head, inner, tail = spec[0...open], spec[(open + 1)...close], spec[(close + 1)..]
  expansions(head + tail) + expansions(inner).flat_map { |i| expansions(head + i + tail) }
end

Rails.application.reload_routes! if Rails.application.respond_to?(:reload_routes!)
out = File.open(ENV.fetch("ORACLE_OUT"), "w")
Rails.application.routes.routes.each do |route|
  spec = route.path.spec.to_s.sub(/\(\.:format\)\z/, "")
  verbs = route.verb.to_s.empty? ? ["ANY"] : route.verb.to_s.split("|")
  endpoint =
    if route.internal
      "INTERNAL"
    elsif route.app.respond_to?(:engine?) && route.app.engine?
      "ENGINE"
    elsif route.app.app.is_a?(ActionDispatch::Routing::Redirect)
      "REDIRECT"
    elsif route.defaults[:controller]
      # The class, as roundhouse names it: Rails' path spelling follows the route's `module`.
      "#{"#{route.defaults[:controller]}_controller".camelize}##{route.defaults[:action]}"
    else
      "RACK"
    end
  name = route.name.to_s.empty? ? "-" : route.name
  expansions(spec).uniq.each do |path|
    path = "/" if path.empty?
    verbs.each { |verb| out.puts "#{verb} #{path} #{endpoint} #{name}" }
  end
end
out.close

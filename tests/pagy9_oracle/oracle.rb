# Oracle: pagy 9.3.5 (the version maybe's Gemfile.lock pins), driven as a
# controller drives it: Pagy::Backend#pagy(scope, **vars) and, with the array
# extra, #pagy_array(array, **vars), over a request (path + query string), then
# the readers, `series` and Pagy::Frontend#pagy_url_for the views call. Prints
# one JSON object per case. `MODE` picks the extras: "plain" (none), or
# "overflow:<mode>" (the overflow and array extras, `Pagy::DEFAULT[:overflow]`
# set to <mode>, as maybe's initializer does).
#
# run: MODE=plain ruby -I gems/gems/pagy-9.3.5/lib oracle.rb > plain.jsonl
require "json"
require "active_record"
require "rack"
require "pagy"
mode = ENV.fetch("MODE")
if mode.start_with?("overflow:")
  require "pagy/extras/overflow"
  require "pagy/extras/array"
  Pagy::DEFAULT[:overflow] = mode.split(":", 2)[1].to_sym
end

ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
ActiveRecord::Schema.verbose = false
ActiveRecord::Schema.define { create_table(:items) { |t| t.string :name } }
class Item < ActiveRecord::Base; end

class FakeController
  include Pagy::Backend
  include Pagy::Frontend
  def initialize(path, query) = (@path = path; @query = query)
  def params = Rack::Utils.parse_nested_query(@query).transform_keys(&:to_sym)
  def request
    Struct.new(:base_url, :path, :GET).new("http://example.org", @path, Rack::Utils.parse_nested_query(@query))
  end
  public :pagy
  public :pagy_array if ENV["MODE"] != "plain"
end

def run(count, path, query, array: false, **opts)
  Item.delete_all
  Item.insert_all((1..count).map { |i| { id: i, name: "n#{i}" } }) if count > 0
  c = FakeController.new(path, query)
  base = { "mode" => ENV["MODE"], "count" => count, "path" => path, "query" => query, "array" => array, "opts" => opts.transform_keys(&:to_s) }
  begin
    pagy, records = array ? c.pagy_array((1..count).to_a, **opts) : c.pagy(Item.order(:id), **opts)
  rescue StandardError => e
    return puts JSON.generate(base.merge("error" => e.class.name, "message" => e.message))
  end
  series = pagy.series
  urls = (series.reject { |s| s == :gap } + [pagy.prev, pagy.next].compact).to_h { |p| [p.to_s, c.pagy_url_for(pagy, p)] }
  out = base.merge(
    "page" => pagy.page, "limit" => pagy.limit, "last" => pagy.last, "pages" => pagy.pages,
    "total" => pagy.count, "offset" => pagy.offset, "from" => pagy.from, "to" => pagy.to, "in" => pagy.in,
    "prev" => pagy.prev, "next" => pagy.next,
    "series" => series.map { |s| s.is_a?(Symbol) ? ":#{s}" : s },
    "items_var" => pagy.vars[:items],
    "ids" => array ? records : records.pluck(:id),
    "urls" => urls,
  )
  puts JSON.generate(out)
end

[0, 1, 20, 21, 45, 131].each do |n|
  [nil, "2", "3", "7"].each do |pg|
    run(n, "/articles", pg ? "page=#{pg}" : "")
  end
end
# deep pages: gaps on the left, right, both; past the end
[10, 40, 75, 76, 77, 80, 81, 9999].each { |pg| run(400, "/articles", "page=#{pg}", limit: 5) }
run(45, "/articles", "page=4")
# a page that is not a positive integer
["page=0", "page=-3", "page=abc", "page=2abc", "page=", "page=2&page=3", "page=+2", "page=0x3"].each { |q| run(45, "/articles", q) }
# limit: as the apps pass it (a String from the query, an Integer, a blank, nil)
[["10", ""], ["10", "page=2"], [10, "page=3"], ["abc", ""], ["0", ""], ["", "page=2"], [nil, "page=2"], ["7x", "page=2"], [-1, ""]].each do |limit, q|
  run(45, "/articles", q, limit: limit)
end
# explicit page: beats the param, as a String or an Integer, or a nil that falls back to it
[[4, "page=2"], ["3", ""], [nil, "page=2"], ["", "page=2"], ["abc", ""]].each { |pg, q| run(100, "/articles", q, page: pg, limit: 10) }
# items: is kept in vars and read by nothing else
run(45, "/articles", "", items: 5)
run(45, "/articles", "page=2", items: 50, limit: 10)
# the links: existing query keys kept, in order; page replaced in place or appended
run(100, "/search", "q=ruby+gems&sort=top&page=2", limit: 10)
run(100, "/search", "page=2&q=ruby", limit: 10)
run(100, "/search", "q=a%26b%3Dc&topic=x%2Fy", limit: 10)
run(100, "/search", "q=h%C3%A9llo+w%C3%B6rld&page=3", limit: 10)
run(100, "/topics/ruby-on-rails", "sort=new", limit: 10)
run(100, "/articles", "flag&page=2", limit: 10)
run(100, "/articles", "empty=&page=2", limit: 10)
run(100, "/articles", "page=2&limit=3", limit: 10)
run(100, "/articles", "per_page=20&page=2", limit: "20")
run(100, "/articles", "x=%3Cb%3E&page=2", limit: 10)
run(100, "/articles", "a=1&a=2&page=2", limit: 10)
run(100, "/articles", "a+b=c+d&page=2", limit: 10)
run(100, "/articles", "a=%2B+%20%7E%2A&page=2", limit: 10)
run(100, "/articles", "k[x]=1&page=2", limit: 10)
# the array extra
if ENV["MODE"] != "plain"
  [[0, ""], [7, ""], [7, "page=2"], [7, "page=9"], [45, "page=3"]].each { |n, q| run(n, "/rows", q, array: true, limit: 3) }
  run(45, "/rows", "page=2", array: true, limit: "10")
end

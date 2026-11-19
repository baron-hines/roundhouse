//! The pagy gem before 43 (9.3.5, maybe's lock) on real-blog:
//! `include Pagy::Backend` in the controller and `Pagy::Frontend` in the
//! helper, `@pagy, @articles = pagy(scope, ...)` over a seeded table, and a
//! view that walks `@pagy.series` and links with `pagy_url_for`, as maybe's
//! `shared/_pagination` does.
//!
//! Every expected value is what the REAL gem answered for the same call:
//! `tests/pagy9_oracle/oracle.rb` drives pagy 9.3.5 on ActiveRecord + sqlite
//! and Rack, without extras and with the overflow and array extras in each
//! overflow mode, and prints `tests/pagy9_oracle/cases.jsonl`.

use super::emit_and_run;

const CASES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/pagy9_oracle/cases.jsonl");

/// The view: rows, then the series and prev/next links, then the readers.
const VIEW: &str = r#"
  <div id="nav"><% if @pagy.prev %><a href="<%= pagy_url_for(@pagy, @pagy.prev) %>">prev</a><% end %><% @pagy.series.each do |item| %><% if item.is_a?(Integer) %><a href="<%= pagy_url_for(@pagy, item) %>"><%= item %></a><% elsif item.is_a?(String) %><a href="<%= pagy_url_for(@pagy, item) %>" class="current"><%= item %></a><% elsif item == :gap %><span>...</span><% end %><% end %><% if @pagy.next %><a href="<%= pagy_url_for(@pagy, @pagy.next) %>">next</a><% end %></div>
  <div id="info"><%= @pagy.page %> <%= @pagy.limit %> <%= @pagy.count %> <%= @pagy.pages %> <%= @pagy.from %> <%= @pagy.to %> <%= @pagy.in %> <%= @pagy.vars[:items].inspect %></div>
</div>"#;

/// real-blog with pagy 9.3.5 locked, `ArticlesController#index` calling
/// `pagy_call` over `Article.order(:id)`, and `initializer` as
/// `config/initializers/pagy.rb` when there is one.
fn app(pagy_call: &str, initializer: Option<&str>) -> emit_and_run::Overlay {
    let overlay = emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    pagy (9.3.5)\n    debug (1.11.1)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  pagy\n")
        .edit(
            "app/controllers/application_controller.rb",
            "class ApplicationController < ActionController::Base\n",
            "class ApplicationController < ActionController::Base\n  include Pagy::Backend\n",
        )
        .edit("app/helpers/application_helper.rb", "module ApplicationHelper\n", "module ApplicationHelper\n  include Pagy::Frontend\n")
        .edit(
            "app/controllers/articles_controller.rb",
            "@articles = Article.includes(:comments).order(created_at: :desc)",
            &format!("scope = Article.order(:id)\n    @pagy, @articles = {pagy_call}"),
        )
        .edit("app/views/articles/index.html.erb", "    <% end %>\n  </div>\n</div>", &format!("    <% end %>\n  </div>{VIEW}"));
    match initializer {
        Some(src) => overlay.write("config/initializers/pagy.rb", src),
        None => overlay,
    }
}

const OVERFLOW_LAST_PAGE: &str = "require \"pagy/extras/overflow\"\nrequire \"pagy/extras/array\"\n\nPagy::DEFAULT[:overflow] = :last_page\n";

const HELPERS: &str = r##"
require "json"
require "erb"
CASES = File.readlines("__CASES__").map { |l| JSON.parse(l) }
def get(path, query)
  status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => path, "QUERY_STRING" => query, "rack.input" => StringIO.new(""))
  [status, body.join]
end
def div(body, id)
  m = body.match(%r{<div id="#{id}">(.*?)</div>}m)
  m && m[1]
end
def titles(body)
  body.scan(/>(Post \d+)</).flatten
end
def seed(n)
  Article.delete_all
  n.times { |i| Article.create!(title: "Post #{i + 1}", body: "body number #{i + 1} is long enough") }
end
def case_for(mode, count, query, opts)
  CASES.find { |c| c["mode"] == mode && !c["array"] && c["count"] == count && c["path"] == "/articles" && c["query"] == query && c["opts"] == opts } or raise "no oracle case for #{mode} #{count} #{query.inspect} #{opts.inspect}"
end
# The view's markup for an oracle case.
def nav(c)
  h = ->(s) { ERB::Util.html_escape(s) }
  out = +""
  out << %(<a href="#{h.(c["urls"][c["prev"].to_s])}">prev</a>) if c["prev"]
  c["series"].each do |item|
    out << case item
           when Integer then %(<a href="#{h.(c["urls"][item.to_s])}">#{item}</a>)
           when ":gap" then "<span>...</span>"
           else %(<a href="#{h.(c["urls"][item])}" class="current">#{item}</a>)
           end
  end
  out << %(<a href="#{h.(c["urls"][c["next"].to_s])}">next</a>) if c["next"]
  out
end
def info(c)
  [c["page"], c["limit"], c["total"], c["pages"], c["from"], c["to"], c["in"], c["items_var"].inspect].join(" ")
end
def check(mode, count, query, opts, want_rows)
  status, body = get("/articles", query)
  raise "GET /articles?#{query} answered #{status}" unless status == 200
  c = case_for(mode, count, query, opts)
  raise "#{query}: rows #{titles(body).inspect}, want #{want_rows.inspect}" unless titles(body) == want_rows
  raise "#{query}: nav\n#{div(body, "nav")}\nwant\n#{nav(c)}" unless div(body, "nav") == nav(c)
  raise "#{query}: info #{div(body, "info").inspect}, want #{info(c).inspect}" unless div(body, "info") == info(c)
end
"##;

fn script(body: &str) -> String {
    format!("{}\n{}", HELPERS.replace("__CASES__", CASES), body)
}

/// No extras: the pages, the links with the other query pairs kept, and
/// a page past the end or not a positive integer raising the gem's error
/// out of the dispatcher, as Rails' test environment shows it.
#[test]
fn pages_and_links_without_extras() {
    let run = app("pagy(scope, limit: params[:per_page] || \"10\")", None).run_ruby(&script(
        r##"
seed(45)
check("plain", 45, "", { "limit" => "10" }, (1..10).map { |i| "Post #{i}" })
check("plain", 45, "page=2", { "limit" => "10" }, (11..20).map { |i| "Post #{i}" })
{ "page=6" => Pagy::OverflowError, "page=0" => Pagy::VariableError, "page=abc" => Pagy::VariableError,
  "page=" => Pagy::VariableError, "per_page=abc" => Pagy::VariableError, "per_page=0" => Pagy::VariableError }.each do |query, error|
  begin
    status, = get("/articles", query)
    raise "GET /articles?#{query} answered #{status}, want #{error}"
  rescue Pagy::VariableError => e
    raise "GET /articles?#{query} raised #{e.class}, want #{error}" unless e.class == error
  end
end
seed(100)
check("plain", 100, "per_page=20&page=2", { "limit" => "20" }, (21..40).map { |i| "Post #{i}" })
puts "PASS"
"##,
    ));
    run.assert_passes();
}

/// maybe's initializer: the overflow extra in `:last_page` mode serves the
/// last page past the end, and `items:` is only kept in `vars`.
#[test]
fn the_overflow_extra_serves_the_last_page() {
    let run = app("pagy(scope, items: 5)", Some(OVERFLOW_LAST_PAGE)).run_ruby(&script(
        r##"
seed(45)
check("overflow:last_page", 45, "", { "items" => 5 }, (1..20).map { |i| "Post #{i}" })
status, body = get("/articles", "page=9")
raise "page=9 answered #{status}" unless status == 200
raise "page=9 rows #{titles(body).inspect}" unless titles(body) == (41..45).map { |i| "Post #{i}" }
raise "page=9 info #{div(body, "info").inspect}" unless div(body, "info") == "3 20 45 3 41 45 5 5"
puts "PASS"
"##,
    ));
    run.assert_passes();
}

/// Every non-array call the oracle recorded, against `Pagy` directly: the
/// paginator state, the series and the links equal the gem's in each
/// overflow mode, and the gem's errors are raised by class and message.
#[test]
fn the_oracle_matrix() {
    let run = app("pagy(scope)", Some(OVERFLOW_LAST_PAGE)).run_ruby(&script(
        r##"
checked = 0
CASES.each do |c|
  next if c["array"]
  opts = c["opts"]
  Pagy.overflow = c["mode"] == "plain" ? nil : c["mode"].split(":")[1].to_sym
  query = c["query"]
  raw_page = opts.key?("page") ? opts["page"] : nil
  if raw_page.nil?
    pair = query.split("&").map { |s| s.split("=", 2) }.select { |k, _| Pagy.decode(k.to_s) == "page" }.last
    raw_page = pair.nil? ? 1 : Pagy.decode(pair[1].to_s).to_i
  end
  begin
    pagy = Pagy.new(c["count"], opts["limit"], raw_page, opts["items"], c["path"], query)
  rescue Pagy::VariableError => e
    raise "#{c.inspect}: raised #{e.class} #{e.message}" unless c["error"] == e.class.name && c["message"] == e.message
    checked += 1
    next
  end
  raise "#{c.inspect}: did not raise" if c["error"]
  got = { "page" => pagy.page, "limit" => pagy.limit, "last" => pagy.last, "pages" => pagy.pages, "total" => pagy.count,
          "offset" => pagy.offset, "from" => pagy.from, "to" => pagy.to, "in" => pagy.in, "prev" => pagy.prev, "next" => pagy.next,
          "series" => pagy.series.map { |s| s.is_a?(Symbol) ? ":#{s}" : s }, "items_var" => pagy.vars[:items] }
  got.each { |k, v| raise "#{c.inspect}: #{k} got #{v.inspect}" unless v == c[k] }
  unless query.include?("[")
    links = (pagy.series.reject { |s| s == :gap } + [pagy.prev, pagy.next].compact).to_h { |p| [p.to_s, pagy.url_for(p)] }
    raise "#{c.inspect}: urls got #{links.inspect}" unless links == c["urls"]
  end
  checked += 1
end
raise "only #{checked} cases checked" unless checked > 250
puts "PASS #{checked}"
"##,
    ));
    run.assert_passes();
}

/// Spinel: the paginator state, the series and the links for a handful of
/// the oracle's cases (the values below are copied from
/// `tests/pagy9_oracle/cases.jsonl`), in each overflow mode. The app's
/// initializer replays `:last_page` at boot, which the script resets.
const SPINEL_ASSERTIONS: &str = r##"
def expect(label, got, want)
  raise "#{label}: got #{got.inspect}, want #{want.inspect}" unless got == want
end
def state(p)
  [p.page, p.limit, p.last, p.from, p.to, p.in, p.prev, p.next]
end

p = Pagy.new(400, 5, 40, nil, "/articles", "page=40")
expect("deep", state(p), [40, 5, 80, 196, 200, 5, 39, 41])
expect("deep series", p.series, [1, :gap, 39, "40", 41, :gap, 80])
expect("deep link", p.url_for(41), "/articles?page=41")

p = Pagy.new(100, 10, 3, nil, "/search", "q=h%C3%A9llo+w%C3%B6rld&page=3")
expect("escaped series", p.series, [1, 2, "3", 4, 5, :gap, 10])
expect("escaped link", p.url_for(10), "/search?q=h%C3%A9llo+w%C3%B6rld&page=10")

p = Pagy.new(100, 10, 2, nil, "/articles", "a=1&a=2&page=2")
expect("repeated key", p.url_for("2"), "/articles?a=2&page=2")

p = Pagy.new(45, 10, 2, 50, "/articles", "page=2")
expect("items", [p.vars[:items], p.vars[:limit], p.vars[:count]], [50, 10, 45])

raised = ""
begin
  Pagy.new(45, "abc", 1, nil, "/articles", "")
rescue Pagy::VariableError => e
  raised = e.message
end
expect("limit abc", raised, "expected :limit >= 1; got \"abc\"")

Pagy.overflow = nil
raised = ""
begin
  Pagy.new(45, nil, 7, nil, "/articles", "page=7")
rescue Pagy::OverflowError => e
  raised = e.message
end
expect("overflow", raised, "expected :page in 1..3; got 7")

Pagy.overflow = :last_page
p = Pagy.new(45, nil, 7, nil, "/articles", "page=7")
expect("last_page", state(p), [3, 20, 3, 41, 45, 5, 2, nil])
expect("last_page series", p.series, [1, 2, "3"])

Pagy.overflow = :empty_page
p = Pagy.new(400, 5, 81, nil, "/articles", "page=81")
expect("empty_page", state(p), [81, 5, 80, 0, 0, 0, 80, nil])
expect("empty_page series", p.series, [1, :gap, 76, 77, 78, 79, 80])
puts "pagy9 spinel passed"
"##;

#[test]
fn the_paginator_on_native_ruby() {
    let run = app("pagy(scope)", Some(OVERFLOW_LAST_PAGE)).run_ruby(SPINEL_ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("pagy9 spinel passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn the_paginator_on_spinel() {
    let run = app("pagy(scope)", Some(OVERFLOW_LAST_PAGE)).run_spinel(SPINEL_ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("pagy9 spinel passed"), "{}", run.stdout);
}

/// `Pagy::DEFAULT[:limit] = 15` is the default page size; another extra or
/// another `Pagy::DEFAULT` key is reported, since the emitted app would
/// paginate without it.
#[test]
fn the_initializer_settings() {
    let run = app("pagy(scope)", Some("Pagy::DEFAULT[:limit] = 15\n")).run_ruby(&script(
        r##"
seed(45)
status, body = get("/articles", "page=2")
raise "answered #{status}" unless status == 200
raise "rows #{titles(body).inspect}" unless titles(body) == (16..30).map { |i| "Post #{i}" }
puts "PASS"
"##,
    ));
    run.assert_passes();
    let (_emitted, errors) = app("pagy(scope)", Some("require \"pagy/extras/bootstrap\"\nPagy::DEFAULT[:size] = 9\n"))
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("pagy/extras/bootstrap")), "{errors:#?}");
    assert!(errors.iter().any(|e| e.contains("Pagy::DEFAULT[:size]")), "{errors:#?}");
}

//! `respond_with` from the responders gem, for a controller whose class-level `respond_to` is json
//! alone: what the Responder's `to_format` path answers. The expected table is the answer of the
//! real gem (responders 3.2.1 under Rails 8.1) to the same requests.

use super::emit_and_run;

const CONTROLLER: &str = r#"class GadgetsController < ApplicationController
  skip_forgery_protection
  respond_to :json

  def show
    @article = Article.new(title: "s", body: "a body long enough to pass")
    respond_with @article
  end

  def tmpl
    @article = Article.new(title: params[:name], body: "a body long enough to pass")
    @article.valid?
    respond_with @article
  end

  def create
    @article = Article.new(title: params[:name], body: "a body long enough to pass")
    @article.valid?
    respond_with @article, location: "/done"
  end

  def empty
    respond_with({}, location: "/sent")
  end

  def update
    @article = Article.new(title: params[:name], body: "a body long enough to pass")
    @article.valid?
    respond_with @article, location: "/done"
  end

  def destroy
    @article = Article.new(title: params[:name], body: "a body long enough to pass")
    @article.valid?
    respond_with @article, location: "/done"
  end

  def stat
    @article = Article.new(title: "x", body: "a body long enough to pass")
    respond_with @article, status: :accepted, location: "/done"
  end

  def typed
    @article = Article.new(title: params[:name], body: "a body long enough to pass")
    @article.valid?
    respond_with @article, location: "/done", content_type: "application/vnd.gadget+json"
  end

  def blk
    @article = Article.new(title: params[:name], body: "a body long enough to pass")
    @article.valid?
    respond_with(@article, location: "/done") { |format| format.html { render plain: "custom" } }
  end

  def act
    @article = Article.new
    @article.valid?
    respond_with @article, action: :edit, location: "/done"
  end

  def helper_call
    answer
  end

  def several
    @article = Article.new(title: "x", body: "a body long enough to pass")
    @other = Article.new(title: "y", body: "a body long enough to pass")
    respond_with @article, @other
  end

  private

  def answer
    @article = Article.new(title: "h", body: "a body long enough to pass")
    respond_with @article
  end
end
"#;

// `tmpl`'s template is `gadgets/tmpl.json.jbuilder`, the parent's: Rails' lookup walks the controller's prefixes.
const SUB_CONTROLLER: &str = r#"class SubGadgetsController < GadgetsController
  def tmpl
    @article = Article.new(title: params[:name], body: "a body long enough to pass")
    respond_with @article
  end
end
"#;

const ROUTES: &str = r#"  get "g/show", to: "gadgets#show"
  get "g/tmpl", to: "gadgets#tmpl"
  post "g/tmpl", to: "gadgets#tmpl"
  post "g/create", to: "gadgets#create"
  post "g/empty", to: "gadgets#empty"
  put "g/update", to: "gadgets#update"
  patch "g/update", to: "gadgets#update"
  delete "g/destroy", to: "gadgets#destroy"
  post "g/stat", to: "gadgets#stat"
  post "g/blk", to: "gadgets#blk"
  post "g/typed", to: "gadgets#typed"
  post "g/act", to: "gadgets#act"
  post "g/several", to: "gadgets#several"
  get "g/helper_call", to: "gadgets#helper_call"
  get "g/sub_tmpl", to: "sub_gadgets#tmpl"
"#;

#[test]
fn respond_with_answers_as_the_responder_does_for_json() {
    let run = emit_and_run::real_blog()
        .write("app/controllers/gadgets_controller.rb", CONTROLLER)
        .write("app/views/gadgets/tmpl.json.jbuilder", "json.tmpl @article.title\n")
        .write("app/views/gadgets/helper_call.json.jbuilder", "json.helper @article.title\n")
        .write("app/controllers/sub_gadgets_controller.rb", SUB_CONTROLLER)
        .edit("config/routes.rb", "  root \"articles#index\"\n", &format!("  root \"articles#index\"\n{ROUTES}"))
        .run_ruby(
            r##"
def hit(verb, path, query = "", accept = "application/json")
  env = { "REQUEST_METHOD" => verb, "PATH_INFO" => path, "QUERY_STRING" => query, "REMOTE_ADDR" => "127.0.0.1",
          "HTTP_HOST" => "localhost", "rack.input" => StringIO.new(""), "HTTP_ACCEPT" => accept }
  status, headers, body = Main.run_rack(env)
  [status, headers["location"] || headers["Location"], body.join.strip]
end
# verb, path, query, accept, status, location, body substring
[
  ["GET",    "/g/show",    "",       "json", 200, nil,     "\"title\":\"s\""],
  ["GET",    "/g/tmpl",    "name=t", "json", 200, nil,     "{\"tmpl\":\"t\"}"],
  ["POST",   "/g/tmpl",    "name=t", "json", 200, nil,     "{\"tmpl\":\"t\"}"],
  ["POST",   "/g/tmpl",    "",       "json", 422, nil,     "{\"errors\":{\"title\":"],
  ["POST",   "/g/create",  "name=a", "json", 201, "/done", "\"title\":\"a\""],
  ["POST",   "/g/create",  "",       "json", 422, nil,     "{\"errors\":{\"title\":"],
  ["POST",   "/g/empty",   "",       "json", 201, "/sent", "{}"],
  ["PUT",    "/g/update",  "name=a", "json", 204, "/done", ""],
  ["PATCH",  "/g/update",  "name=a", "json", 204, "/done", ""],
  ["PUT",    "/g/update",  "",       "json", 422, nil,     "{\"errors\":{\"title\":"],
  ["DELETE", "/g/destroy", "name=a", "json", 204, "/done", ""],
  ["DELETE", "/g/destroy", "",       "json", 422, nil,     "{\"errors\":{\"title\":"],
  ["POST",   "/g/stat",    "",       "json", 202, "/done", "\"title\":\"x\""],
  ["POST",   "/g/blk",     "name=a", "json", 201, "/done", "\"title\":\"a\""],
  ["GET",    "/g/show",    "",       "html", 406, nil,     ""],
  ["POST",   "/g/blk",     "name=a", "html", 200, nil,     "custom"],
  # A bare `*/*` with no explicit format negotiates to json; with `.html`, or with no Accept at all, it does not.
  ["GET",    "/g/show",      "",       "any",  200, nil,     "\"title\":\"s\""],
  ["POST",   "/g/blk",       "name=a", "any",  201, "/done", "\"title\":\"a\""],
  ["PUT",    "/g/update",    "",       "any",  422, nil,     "{\"errors\":{\"title\":"],
  ["GET",    "/g/show.html", "",       "any",  406, nil,     ""],
  ["GET",    "/g/show",      "",       "none", 406, nil,     ""],
].each do |verb, path, query, accept, status, location, body|
  # The native dispatchers take a request's format from the path's extension, not from Accept.
  got = hit(verb, accept == "json" ? path + ".json" : path, query, { "json" => "application/json", "html" => "text/html", "any" => "*/*", "none" => "" }[accept])
  ok = got[0] == status && got[1].to_s.sub("http://localhost", "") == location.to_s && got[2].include?(body)
  raise "#{verb} #{path}?#{query} (#{accept}): got #{got.inspect}, want #{[status, location, body].inspect}" unless ok
end
# The call's other options reach the resource's render, not the errors' (`display_errors`).
[["name=a", 201, "application/vnd.gadget+json"], ["", 422, "application/json"]].each do |query, status, type|
  env = { "REQUEST_METHOD" => "POST", "PATH_INFO" => "/g/typed.json", "QUERY_STRING" => query, "REMOTE_ADDR" => "127.0.0.1",
          "HTTP_HOST" => "localhost", "rack.input" => StringIO.new(""), "HTTP_ACCEPT" => "application/json" }
  got_status, headers, _ = Main.run_rack(env)
  got_type = (headers["content-type"] || headers["Content-Type"]).to_s.split(";").first
  raise "typed?#{query}: got #{[got_status, got_type].inspect}, want #{[status, type].inspect}" unless got_status == status && got_type == type
end
# Not modeled: these must fail, not answer with a response Rails wouldn't give.
# A subclass's action whose json template sits under the parent's views: Rails renders it, so the call stays.
got = begin
  hit("GET", "/g/sub_tmpl.json", "name=t")
rescue ArgumentError, NoMethodError, NameError
  [500]
end
raise "sub_tmpl: expected a failure, got #{got.inspect}" unless got[0] == 500
# A helper's `respond_with` would render the calling action's template, which the lowering can't name.
[["POST", "/g/act"], ["POST", "/g/several"], ["GET", "/g/helper_call"]].each do |verb, path|
  got = begin
    hit(verb, path + ".json")
  rescue ArgumentError, NoMethodError, NameError
    [500]
  end
  raise "#{path}: expected a failure, got #{got.inspect}" unless got[0] == 500
end
puts "respond_with json passed"
"##,
        );
    run.assert_passes();
    assert!(run.stdout.contains("respond_with json passed"), "{}", run.stdout);
}

const API_CONTROLLER: &str = r#"class BareGadgetsController < ActionController::API
  include ActionController::MimeResponds
  respond_to :json

  def show
    @article = Article.new(title: "s", body: "a body long enough to pass")
    respond_with @article
  end

  def stat
    @article = Article.new(title: "x", body: "a body long enough to pass")
    respond_with @article, status: :accepted, location: "/done"
  end

  def update
    @article = Article.new(title: params[:name], body: "a body long enough to pass")
    @article.valid?
    respond_with @article, location: "/done"
  end
end
"#;

const API_ROUTES: &str = r#"  get "b/show", to: "bare_gadgets#show"
  post "b/stat", to: "bare_gadgets#stat"
  put "b/update", to: "bare_gadgets#update"
"#;

/// `ActionController::API` without jbuilder renders no views: a 204 carries no location, and a bare `*/*`
/// answers a blank body with the call's status and location (responders 3.2.1 under Rails 8.1.4.1).
#[test]
fn respond_with_on_an_api_controller_without_views() {
    let run = emit_and_run::real_blog()
        .remove("app/views/articles/index.json.jbuilder")
        .remove("app/views/articles/show.json.jbuilder")
        .remove("app/views/articles/_article.json.jbuilder")
        .edit("Gemfile", "gem \"jbuilder\"\n", "")
        .edit("Gemfile.lock", "    jbuilder (2.15.1)\n", "")
        .edit("Gemfile.lock", "  jbuilder\n", "")
        .write("app/controllers/bare_gadgets_controller.rb", API_CONTROLLER)
        .edit("config/routes.rb", "  root \"articles#index\"\n", &format!("  root \"articles#index\"\n{API_ROUTES}"))
        .run_ruby(
            r##"
def hit(verb, path, query, accept)
  env = { "REQUEST_METHOD" => verb, "PATH_INFO" => path, "QUERY_STRING" => query, "REMOTE_ADDR" => "127.0.0.1",
          "HTTP_HOST" => "localhost", "rack.input" => StringIO.new(""), "HTTP_ACCEPT" => accept }
  status, headers, body = Main.run_rack(env)
  [status, (headers["location"] || headers["Location"]).to_s.sub("http://localhost", ""), body.join]
end
# verb, path, query, accept, status, location, body (exact when blank, else a substring)
[
  ["GET",  "/b/show.json",   "",       "application/json", 200, "",      "\"title\":\"s\""],
  ["PUT",  "/b/update.json", "name=a", "application/json", 204, "",      ""],
  ["PUT",  "/b/update.json", "",       "application/json", 422, "",      "{\"errors\":{\"title\":"],
  ["GET",  "/b/show",        "",       "*/*",              200, "",      " "],
  ["POST", "/b/stat",        "",       "*/*",              202, "/done", " "],
  ["PUT",  "/b/update",      "",       "*/*",              200, "/done", " "],
].each do |verb, path, query, accept, status, location, body|
  got = hit(verb, path, query, accept)
  ok = got[0] == status && got[1] == location && (body.strip.empty? ? got[2] == body : got[2].include?(body))
  raise "#{verb} #{path}?#{query} (#{accept}): got #{got.inspect}, want #{[status, location, body].inspect}" unless ok
end
puts "respond_with api passed"
"##,
        );
    run.assert_passes();
    assert!(run.stdout.contains("respond_with api passed"), "{}", run.stdout);
}

/// `require: false` keeps jbuilder's railtie from running, so the API controller renders no views.
#[test]
fn respond_with_on_an_api_controller_when_jbuilder_is_not_required() {
    let run = emit_and_run::real_blog()
        .remove("app/views/articles/index.json.jbuilder")
        .remove("app/views/articles/show.json.jbuilder")
        .remove("app/views/articles/_article.json.jbuilder")
        .edit("Gemfile", "gem \"jbuilder\"\n", "gem \"jbuilder\", require: false\n")
        .write("app/controllers/bare_gadgets_controller.rb", API_CONTROLLER)
        .edit("config/routes.rb", "  root \"articles#index\"\n", &format!("  root \"articles#index\"\n{API_ROUTES}"))
        .run_ruby(
            r##"
env = { "REQUEST_METHOD" => "GET", "PATH_INFO" => "/b/show", "QUERY_STRING" => "", "REMOTE_ADDR" => "127.0.0.1",
        "HTTP_HOST" => "localhost", "rack.input" => StringIO.new(""), "HTTP_ACCEPT" => "*/*" }
status, _, body = Main.run_rack(env)
raise "got #{[status, body.join].inspect}" unless status == 200 && body.join == " "
puts "respond_with api unrequired passed"
"##,
        );
    run.assert_passes();
    assert!(run.stdout.contains("respond_with api unrequired passed"), "{}", run.stdout);
}

/// An app that bundles jbuilder renders views on `ActionController::API` even with no `.jbuilder` file of its
/// own: a bare `*/*` gets the json answers and errors their 422, as on `ActionController::Base`.
#[test]
fn respond_with_on_an_api_controller_of_an_app_that_bundles_jbuilder() {
    let run = emit_and_run::real_blog()
        .remove("app/views/articles/index.json.jbuilder")
        .remove("app/views/articles/show.json.jbuilder")
        .remove("app/views/articles/_article.json.jbuilder")
        .write("app/controllers/bare_gadgets_controller.rb", API_CONTROLLER)
        .edit("config/routes.rb", "  root \"articles#index\"\n", &format!("  root \"articles#index\"\n{API_ROUTES}"))
        .run_ruby(
            r##"
def hit(verb, path, query, accept)
  env = { "REQUEST_METHOD" => verb, "PATH_INFO" => path, "QUERY_STRING" => query, "REMOTE_ADDR" => "127.0.0.1",
          "HTTP_HOST" => "localhost", "rack.input" => StringIO.new(""), "HTTP_ACCEPT" => accept }
  status, headers, body = Main.run_rack(env)
  [status, (headers["location"] || headers["Location"]).to_s.sub("http://localhost", ""), body.join]
end
# verb, path, query, accept, status, location, body substring
[
  ["GET",  "/b/show",        "",       "*/*",              200, "",      "\"title\":\"s\""],
  ["POST", "/b/stat",        "",       "*/*",              202, "/done", "\"title\":\"x\""],
  ["PUT",  "/b/update",      "",       "*/*",              422, "",      "{\"errors\":{\"title\":"],
  ["PUT",  "/b/update.json", "name=a", "application/json", 204, "/done", ""],
].each do |verb, path, query, accept, status, location, body|
  got = hit(verb, path, query, accept)
  ok = got[0] == status && got[1] == location && got[2].include?(body)
  raise "#{verb} #{path}?#{query} (#{accept}): got #{got.inspect}, want #{[status, location, body].inspect}" unless ok
end
puts "respond_with api jbuilder passed"
"##,
        );
    run.assert_passes();
    assert!(run.stdout.contains("respond_with api jbuilder passed"), "{}", run.stdout);
}

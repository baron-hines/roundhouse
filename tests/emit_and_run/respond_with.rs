//! `respond_with` from the responders gem, for a controller whose class-level `respond_to` is html
//! alone: what the Responder's navigational path answers. The expected table is the answer of the
//! real gem (responders 3.2.1 under Rails 8.1) to the same requests.

use super::emit_and_run;

const CONTROLLER: &str = r#"class GadgetsController < ApplicationController
  skip_forgery_protection
  respond_to :html

  def new
    @article = Article.new
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

  def made
    @article = Article.new(title: params[:name], body: "a body long enough to pass")
    @article.valid?
    respond_with @article, location: "/done"
  end

  def blk
    @article = Article.new
    @article.valid?
    respond_with(@article) { |format| format.html { render plain: "custom" } }
  end

  def blk_any
    @article = Article.new
    respond_with(@article, status: :accepted) { |format| format.any(:html) { render plain: "any" } }
  end

  def rend
    @article = Article.new
    @article.valid?
    respond_with @article, render: { plain: "r", status: 418 }, location: "/done"
  end

  def act
    @article = Article.new
    @article.valid?
    respond_with @article, action: :edit, location: "/done"
  end

  def stat
    @article = Article.new(title: "x", body: "a body long enough to pass")
    respond_with @article, status: :created, location: "/done"
  end

  def blk_json
    @article = Article.new
    @article.valid?
    respond_with(@article) { |format| format.any(:json) { render plain: "json-arm" } }
  end

  def blk_bare_any
    @article = Article.new
    @article.valid?
    respond_with(@article) { |format| format.any { render plain: "wildcard-arm" } }
  end

  def blk_navigational
    @article = Article.new
    respond_with(@article) { |format| format.any(*navigational_formats) { render plain: "navigational-arm" } }
  end

  def blk_unread
    @article = Article.new
    respond_with(@article) { |format| format.any(*other_formats) { render plain: "unread-arm" } }
  end

  def blk_variant
    @article = Article.new
    respond_with(@article) { |format| format.html { |variant| render plain: "variant-arm" } }
  end

  def blk_both
    @article = Article.new
    respond_with(@article) { |format| format.html { render plain: "html-arm" }; format.json { render plain: "json-arm" } }
  end

  def blk_html_splat
    @article = Article.new
    respond_with(@article) { |format| format.any(:html, *other_formats) { render plain: "splat-arm" } }
  end

  def blk_responder
    @article = Article.new
    respond_with(@article, responder: Article) { |format| format.html { render plain: "responder-arm" } }
  end

  def blk_navigational_call
    @article = Article.new
    respond_with(@article) { |format| format.any(*navigational_formats(other_formats.first)) { render plain: "called-arm" } }
  end

  def blk_argument
    @article = Article.new
    respond_with(@article) { |format| format.any(:json, other_formats.first); format.html { render plain: "argument-arm" } }
  end

  def blk_statement
    @article = Article.new
    respond_with(@article) { |format| @seen = "before"; format.html { render plain: @seen.to_s } }
  end

  def computed
    @article = Article.new(title: "x", body: "a body long enough to pass")
    respond_with @article, plain_options
  end

  def ordered
    @trace = []
    respond_with traced_article, traced_options
  end

  def computed_location
    @article = Article.new(title: "x", body: "a body long enough to pass")
    respond_with @article, location_options
  end

  def several
    @article = Article.new(title: "x", body: "a body long enough to pass")
    @other = Article.new(title: "y", body: "a body long enough to pass")
    respond_with @article, @other
  end

  private

  def navigational_formats(*)
    [:html]
  end

  def other_formats
    [:json]
  end

  def plain_options
    { only: [:title] }
  end

  def location_options
    { location: "/elsewhere" }
  end

  def traced_article
    @trace << "resource"
    Article.new(title: "x", body: "a body long enough to pass")
  end

  def traced_options
    @trace << "options"
    { only: [:title] }
  end
end
"#;

const ROUTES: &str = r#"  get "g/new", to: "gadgets#new"
  post "g/create", to: "gadgets#create"
  post "g/empty", to: "gadgets#empty"
  put "g/update", to: "gadgets#update"
  patch "g/update", to: "gadgets#update"
  delete "g/destroy", to: "gadgets#destroy"
  post "g/made", to: "gadgets#made"
  post "g/blk", to: "gadgets#blk"
  post "g/blk_any", to: "gadgets#blk_any"
  post "g/rend", to: "gadgets#rend"
  post "g/act", to: "gadgets#act"
  post "g/stat", to: "gadgets#stat"
  post "g/blk_json", to: "gadgets#blk_json"
  post "g/nearest", to: "nearest_gadgets#create"
  post "g/blk_bare_any", to: "gadgets#blk_bare_any"
  post "g/blk_navigational", to: "gadgets#blk_navigational"
  post "g/blk_unread", to: "gadgets#blk_unread"
  post "g/blk_variant", to: "gadgets#blk_variant"
  post "g/blk_statement", to: "gadgets#blk_statement"
  post "g/blk_argument", to: "gadgets#blk_argument"
  post "g/blk_navigational_call", to: "gadgets#blk_navigational_call"
  post "g/blk_responder", to: "gadgets#blk_responder"
  post "g/custom", to: "custom_gadgets#create"
  post "g/deep", to: "deep9_gadgets#create"
  post "g/blk_both", to: "gadgets#blk_both"
  post "g/blk_html_splat", to: "gadgets#blk_html_splat"
  post "g/computed", to: "gadgets#computed"
  post "g/ordered", to: "gadgets#ordered"
  post "g/computed_location", to: "gadgets#computed_location"
  post "g/several", to: "gadgets#several"
"#;

#[test]
fn respond_with_answers_as_the_responder_does_for_html() {
    let run = emit_and_run::real_blog()
        .write("app/controllers/gadgets_controller.rb", CONTROLLER)
        .write("app/views/gadgets/new.html.erb", "new-template")
        .write("app/views/gadgets/edit.html.erb", "edit-template")
        .write("app/views/gadgets/made.html.erb", "made-template")
        .write("app/views/gadgets/computed.html.erb", "computed-template")
        .write("app/models/far_responder.rb", "class FarResponder\n  def self.error_status = :bad_request\n  def self.redirect_status = :found\nend\n")
        .write("app/models/near_responder.rb", "class NearResponder\n  def self.error_status = :unprocessable_entity\n  def self.redirect_status = :see_other\nend\n")
        .write("app/controllers/far_gadgets_controller.rb", "class FarGadgetsController < ApplicationController\n  self.responder = FarResponder\n  respond_to :html\nend\n")
        .write("app/controllers/near_gadgets_controller.rb", "class NearGadgetsController < FarGadgetsController\n  self.responder = NearResponder\nend\n")
        .write("app/controllers/nearest_gadgets_controller.rb", "class NearestGadgetsController < NearGadgetsController\n  skip_forgery_protection\n\n  def create\n    @article = Article.new\n    @article.valid?\n    respond_with @article, location: \"/done\"\n  end\nend\n")
        .write("app/views/nearest_gadgets/new.html.erb", "nearest-new")
        .write("app/models/custom_responder.rb", "class CustomResponder\n  def self.error_status = :unprocessable_entity\n  def self.redirect_status = :see_other\n\n  def to_html\n    nil\n  end\nend\n")
        .write("app/controllers/custom_gadgets_controller.rb", "class CustomGadgetsController < ApplicationController\n  skip_forgery_protection\n  self.responder = CustomResponder\n  respond_to :html\n\n  def create\n    @article = Article.new(title: \"x\", body: \"a body long enough to pass\")\n    respond_with @article, location: \"/done\"\n  end\nend\n")
        .write("app/controllers/deep0_gadgets_controller.rb", "class Deep0GadgetsController < ApplicationController\n  respond_to :json\nend\n")
        .write("app/views/gadgets/ordered.html.erb", "<%= @trace.join(\",\") %>")
        .edit("config/routes.rb", "  root \"articles#index\"\n", &format!("  root \"articles#index\"\n{ROUTES}"));
    // An ancestor past the lookup's depth still declares its formats.
    let mut run = run;
    for n in 1..9 {
        run = run.write(&format!("app/controllers/deep{n}_gadgets_controller.rb"), &format!("class Deep{n}GadgetsController < Deep{}GadgetsController\nend\n", n - 1));
    }
    let run = run
        .write("app/controllers/deep9_gadgets_controller.rb", "class Deep9GadgetsController < Deep8GadgetsController\n  skip_forgery_protection\n  respond_to :html\n\n  def create\n    @article = Article.new(title: \"x\", body: \"a body long enough to pass\")\n    respond_with @article, location: \"/done\"\n  end\nend\n")
        .run_ruby(
            r##"
def hit(verb, path, query = "")
  env = { "REQUEST_METHOD" => verb, "PATH_INFO" => path, "QUERY_STRING" => query, "REMOTE_ADDR" => "127.0.0.1",
          "HTTP_HOST" => "localhost", "rack.input" => StringIO.new(""), "HTTP_ACCEPT" => "text/html" }
  status, headers, body = Main.run_rack(env)
  [status, headers["location"] || headers["Location"], body.join.strip]
end
# verb, path, query, status, location, body substring (nil: no check)
[
  ["GET",    "/g/new",     "",       200, nil,     "new-template"],
  ["POST",   "/g/create",  "name=a", 302, "/done", ""],
  ["POST",   "/g/create",  "",       200, "/done", "new-template"],
  ["POST",   "/g/empty",   "",       302, "/sent", ""],
  ["PUT",    "/g/update",  "name=a", 302, "/done", ""],
  ["PUT",    "/g/update",  "",       200, "/done", "edit-template"],
  ["PATCH",  "/g/update",  "",       200, "/done", "edit-template"],
  ["DELETE", "/g/destroy", "name=a", 302, "/done", ""],
  ["DELETE", "/g/destroy", "",       302, "/done", ""],
  ["POST",   "/g/made",    "name=a", 200, "/done", "made-template"],
  ["POST",   "/g/made",    "",       200, "/done", "made-template"],
  ["POST",   "/g/blk",     "",       200, nil,     "custom"],
  ["POST",   "/g/blk_any", "",       200, nil,     "any"],
  ["POST",   "/g/rend",    "",       418, "/done", "r"],
  ["POST",   "/g/act",     "",       200, "/done", "edit-template"],
  ["POST",   "/g/stat",    "",       302, "/done", ""],
  ["POST",   "/g/nearest", "",       422, "/done", "nearest-new"],
  ["POST",   "/g/blk_navigational", "", 200, nil,  "navigational-arm"],
  ["POST",   "/g/computed", "",      200, nil,     "computed-template"],
  ["POST",   "/g/ordered", "",       200, nil,     "resource,options"],
].each do |verb, path, query, status, location, body|
  got = hit(verb, path, query)
  ok = got[0] == status && got[1].to_s.sub("http://localhost", "") == location.to_s && (body.empty? ? true : got[2].include?(body))
  raise "#{verb} #{path}?#{query}: got #{got.inspect}, want #{[status, location, body].inspect}" unless ok
end
# Not modeled: these must fail, not answer with a response Rails wouldn't give.
["/g/computed_location", "/g/several", "/g/blk_unread", "/g/blk_variant", "/g/blk_statement", "/g/blk_argument", "/g/blk_navigational_call", "/g/blk_responder", "/g/custom", "/g/deep", "/g/blk_both", "/g/blk_html_splat", "/g/blk_json", "/g/blk_bare_any"].each do |path|
  got = begin
    hit("POST", path)
  rescue ArgumentError, NoMethodError, NameError
    [500]
  end
  raise "#{path}: expected a failure, got #{got.inspect}" unless got[0] == 500
end
puts "respond_with passed"
"##,
        );
    run.assert_passes();
    assert!(run.stdout.contains("respond_with passed"), "{}", run.stdout);
}

//! A request body that is not form params (a bot posting plain text, as
//! campfire's `Messages::ByBotsController` takes it) reaches the action's
//! `request.body` on the CRuby overlay lane.

const ROUTE_FIND: &str = "  root \"articles#index\"\n";
const ROUTE_REPLACE: &str = "  root \"articles#index\"\n  post \"raw\" => \"articles#raw\"\n";

const ACTION_FIND: &str = "  private\n";
const ACTION_REPLACE: &str = r##"  def raw
    request.body.rewind
    text = request.body.read.force_encoding("UTF-8")
    request.body.rewind
    render plain: "#{text}|#{params[:message].nil? ? "-" : params[:message][:body]}"
  end

  private
"##;

#[test]
fn a_post_body_reaches_request_body_alongside_its_params() {
    super::emit_and_run::real_blog()
        .edit("config/routes.rb", ROUTE_FIND, ROUTE_REPLACE)
        .edit("app/controllers/articles_controller.rb", ACTION_FIND, ACTION_REPLACE)
        .run_ruby(
            r##"
require "stringio"
ActionController::Base.allow_forgery_protection = false
def post_raw(body, type)
  env = {
    "REQUEST_METHOD" => "POST", "PATH_INFO" => "/raw", "QUERY_STRING" => "",
    "CONTENT_TYPE" => type, "CONTENT_LENGTH" => body.bytesize.to_s, "HTTP_ACCEPT" => "text/plain"
  }
  Main.dispatch_core(env, StringIO.new(body))
end
status, text = post_raw("Hello from a bot", "text/plain; charset=utf-8")
raise "plain body: got #{status} #{text.inspect}" unless status == 200 && text == "Hello from a bot|-"
status, text = post_raw("message%5Bbody%5D=hi", "application/x-www-form-urlencoded")
raise "form body: got #{status} #{text.inspect}" unless status == 200 && text == "message%5Bbody%5D=hi|hi"
puts "raw post body reaches request.body"
"##,
        )
        .assert_passes();
}

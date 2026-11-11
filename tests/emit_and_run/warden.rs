//! Warden on the app's own stack (`config.middleware.use Warden::Manager do … end`), compiled from
//! its installed source: a request constraint asks the proxy the middleware put in the Rack env.

#[path = "../support/prerequisite.rs"]
mod prerequisite;
use roundhouse::build_target::BuildTarget;

use super::emit_and_run;

const VERSION: &str = "1.2.9";

fn installed_warden() -> std::path::PathBuf {
    let out = std::process::Command::new("ruby")
        .args(["-e", &format!("print Gem::Specification.find_by_name('warden', '{VERSION}').gem_dir")])
        .output()
        .expect("spawn ruby");
    assert!(out.status.success(), "warden {VERSION} is not installed: {}", String::from_utf8_lossy(&out.stderr));
    std::path::PathBuf::from(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn rb_files(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rb_files(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "rb") {
            out.push(path.strip_prefix(root).unwrap().to_path_buf());
        }
    }
}

const ROUTES: &str = r#"  constraints ->(req) { req.env["warden"].authenticated?(:user) } do
    get "/dashboard", to: "articles#index"
  end
"#;

fn app() -> emit_and_run::Overlay {
    let gem = installed_warden();
    let fixture = roundhouse::fixtures::real_blog();
    let lock = std::fs::read_to_string(fixture.join("Gemfile.lock")).unwrap();
    let lock = lock
        // Not a bare "    rack (": a dependency line ("      rack (>= 2.2.4)") holds it too.
        .replacen("\n    rack (", &format!("\n    warden ({VERSION})\n      rack (>= 2.0.9)\n    rack ("), 1)
        .replacen("DEPENDENCIES\n", "DEPENDENCIES\n  warden\n", 1);
    let gemfile = std::fs::read_to_string(fixture.join("Gemfile")).unwrap() + "gem \"warden\"\n";
    let mut overlay = emit_and_run::real_blog()
        .write("Gemfile", &gemfile)
        .write("Gemfile.lock", &lock)
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .edit(
            "config/application.rb",
            "  class Application < Rails::Application\n",
            "  class Application < Rails::Application\n    config.middleware.use Warden::Manager do |manager|\n      manager.default_scope = :user\n    end\n",
        )
        .edit("config/routes.rb", "  root \"articles#index\"\n", &format!("  root \"articles#index\"\n{ROUTES}"));
    let mut files = Vec::new();
    rb_files(&gem, &gem.join("lib"), &mut files);
    for file in files {
        let text = std::fs::read_to_string(gem.join(&file)).unwrap();
        overlay = overlay.write(&format!("vendor/bundle/ruby/3.4.0/gems/warden-{VERSION}/{}", file.display()), &text);
    }
    overlay
}

/// The app's `use` stacks the gem's class with its block, and the tree requires the gem it names.
#[test]
fn the_apps_own_middleware_use_stacks_warden_with_its_block() {
    let (tree, _) = app().emit(BuildTarget::Ruby);
    let stack = std::fs::read_to_string(tree.join("runtime/gem_middleware.rb")).expect("gem_middleware.rb");
    assert!(stack.contains("Warden::Manager.new(app) { |manager|\nmanager.default_scope = :user\n}"), "{stack}");
    let gemfile = std::fs::read_to_string(tree.join("Gemfile")).expect("Gemfile");
    assert!(gemfile.contains("gem \"warden\", \"= 1.2.9\""), "{gemfile}");
}

/// The constraint asks `env["warden"]`, typed as the `Warden::Proxy` the manager's `call(env)` writes:
/// with no user in the session the route is not there, and the session's user lets it through.
#[test]
fn a_constraint_asks_the_warden_proxy_with_the_session_restored_before_the_routes() {
    let run = app().run_ruby(
        r##"
def hit(path, cookie = nil)
  env = { "REQUEST_METHOD" => "GET", "PATH_INFO" => path, "QUERY_STRING" => "", "REMOTE_ADDR" => "127.0.0.1",
          "HTTP_HOST" => "localhost", "rack.input" => StringIO.new("") }
  env["HTTP_COOKIE"] = cookie if cookie
  Main.run_rack(env)
end
status, = hit("/dashboard")
raise "anonymous: #{status}" unless status == 404
key = Rails.application.session_cookie_key
# Not a record: with no `serialize_from_session`, Warden reads back the stored value itself.
signed = ActionDispatch::Session.signed_cookie("warden.user.user.key=42", key)
status, = hit("/dashboard", "#{key}=#{CgiIo.url_encode(signed)}")
raise "signed in: #{status}" unless status == 200
# The form's CSRF token is written to the session, which is committed once, above Warden.
_, headers, = hit("/articles/new")
session_cookies = Array(headers["set-cookie"]).select { |line| line.start_with?("#{key}=") }
raise "session cookies: #{headers.inspect}" unless session_cookies.length == 1 && session_cookies[0].include?("HttpOnly")
puts "warden passed"
"##,
    );
    run.assert_passes();
    assert!(run.stdout.contains("warden passed"), "{}", run.stdout);
}

/// The same constraint on the Spinel tree, compiled from Warden's source: the request runs through
/// the stack, whose `Warden::Manager` puts its proxy in the env the router asks.
#[test]
#[ignore = "requires the Spinel toolchain"]
fn the_spinel_tree_asks_the_warden_proxy_its_stack_puts_in_the_env() {
    let script = r#"
def answer(session)
  env = { "REQUEST_METHOD" => "GET", "PATH_INFO" => "/dashboard", "rack.session" => session }
  result = GemMiddleware.around(env) do |inner|
    request = ActionDispatch::Request.new
    inner.each { |key, value| request.env[key] = value }
    matched = RouteTable.match_asking("GET", "/dashboard", [RouteTable.root] + RouteTable.table, request)
    [200, {}, [matched.nil? ? "none" : matched.action.to_s]]
  end
  result[2][0].to_s
end
raise "anonymous" unless answer(ActionDispatch::Session.new) == "none"
raise "signed in" unless answer(ActionDispatch::Session.new({ "warden.user.user.key" => "42" })) == "index"
puts "warden on spinel passed"
"#;
    let run = app().run_spinel(script);
    run.assert_passes();
    assert!(run.stdout.contains("warden on spinel passed"), "{}", run.stdout);
}

// Not through an emitted app: the ruby family loads rack's own Rack::Response, so only Spinel runs the port.
#[test]
fn the_spinel_rack_response_port_answers_as_rack_does() {
    let rack = std::process::Command::new("ruby").args(["-rrack", "-e", "1"]).status();
    if !rack.is_ok_and(|s| s.success()) {
        prerequisite::missing("ruby with the rack gem not available");
        return;
    }
    let script = r#"
p Rack::Response.new.finish
p Rack::Response.new("hi", 201, { "X-Own" => "1" }).finish
r = Rack::Response.new(["a", "b"])
p [r.write("cd"), r.length, r.finish]
r = Rack::Response.new
r.redirect("/next")
p [r.status, r.location, r["Location"], r.redirect?, r.successful?, r.finish]
p Rack::Response.new("gone", 204, { "Content-Type" => "text/plain" }).finish
r = Rack::Response.new("x", 404)
r.content_type = "text/html"
p [r.not_found?, r.client_error?, r.content_type, r.has_header?("CONTENT-TYPE"), r.delete_header("content-type"), r.headers]
"#;
    let run = |prelude: String| {
        let out = std::process::Command::new("ruby").args(["-e", &(prelude + script)]).output().expect("spawn ruby");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let port = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/spinel/rack_response.rb");
    let ported = run(format!("load {:?}\n", port.display().to_string()));
    let rack = run("require \"rack\"\n".to_string());
    assert_eq!(ported, rack);
}

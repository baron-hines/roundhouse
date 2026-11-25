//! `config.action_dispatch.cookies_rotations`: lobsters' `cookie_rotator.rb`, which rotates the cookies a pre-7.0
//! Rails wrote (keys derived by SHA1) into the SHA256 ones. A signed and an encrypted cookie minted under the old keys
//! read back, and each goes out again under the current secret; the second request reads those with no Set-Cookie.
//! Rails 8.1.4 answers the same for the same cookies (`rotated_cookies_read_as_rails_reads_them`).

#[path = "../support/prerequisite.rs"]
mod prerequisite;

use super::emit_and_run;

const ROTATOR: &str = r##"Rails.application.config.after_initialize do
  Rails.application.config.action_dispatch.cookies_rotations.tap do |cookies|
    authenticated_encrypted_cookie_salt = Rails.application.config.action_dispatch.authenticated_encrypted_cookie_salt
    signed_cookie_salt = Rails.application.config.action_dispatch.signed_cookie_salt

    key_generator = ActiveSupport::KeyGenerator.new(
      Rails.application.secret_key_base, iterations: 1000, hash_digest_class: OpenSSL::Digest::SHA1
    )
    key_len = ActiveSupport::MessageEncryptor.key_len

    cookies.rotate :encrypted, key_generator.generate_key(authenticated_encrypted_cookie_salt, key_len)
    cookies.rotate :signed, key_generator.generate_key(signed_cookie_salt)
  end
end
"##;

const CONTROLLER: &str = r##"class VisitsController < ApplicationController
  def index
    render plain: "#{cookies.signed[:remember]}|#{cookies.encrypted[:token]}"
  end
end
"##;

/// Both cookies as a pre-7.0 Rails wrote them for `secret`: PBKDF2-SHA1 keys, the signed one under HMAC-SHA1, the
/// encrypted one under AES-256-GCM, each value in the `_rails` envelope for its name.
const MINT: &str = r##"require "openssl"
require "json"
require "base64"
require "cgi"
def mint(secret)
  envelope = ->(value, name) { %({"_rails":{"message":"#{Base64.strict_encode64(JSON.generate(value))}","exp":null,"pur":"cookie.#{name}"}}) }
  signed_key = OpenSSL::PKCS5.pbkdf2_hmac(secret, "signed cookie", 1000, 64, OpenSSL::Digest::SHA1.new)
  payload = Base64.strict_encode64(envelope.("old-signed", "remember"))
  signed = payload + "--" + OpenSSL::HMAC.hexdigest("SHA1", signed_key, payload)
  cipher = OpenSSL::Cipher.new("aes-256-gcm").encrypt
  cipher.key = OpenSSL::PKCS5.pbkdf2_hmac(secret, "authenticated encrypted cookie", 1000, 32, OpenSSL::Digest::SHA1.new)
  iv = cipher.random_iv
  cipher.auth_data = ""
  sealed = cipher.update(envelope.("old-encrypted", "token")) + cipher.final
  [signed, [sealed, iv, cipher.auth_tag].map { |part| Base64.strict_encode64(part) }.join("--")]
end
"##;

const SECRET: &str = "cookie-rotation-secret";

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("app/controllers/visits_controller.rb", CONTROLLER)
        .write("config/initializers/cookie_rotator.rb", ROTATOR)
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/visit.rb", "class Visit < ApplicationRecord\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\n  resources :visits, only: :index\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema[8.1].define(version: 2026_01_01_000000) do\n  create_table \"visits\", force: :cascade do |t|\n    t.string \"room\"\n  end\nend\n",
        )
}

/// The two requests, answered as `body|names of the cookies set` for each: the old cookies read and both go out again,
/// then the new ones read and nothing is set.
const EXPECTED: &str = "old-signed|old-encrypted|remember,token\nold-signed|old-encrypted|\n";

const REQUESTS: &str = r##"
signed, encrypted = mint(Rails.application.secret_key_base)
answer = ""
cookie = "remember=#{CGI.escape(signed)}; token=#{CGI.escape(encrypted)}"
2.times do
  status, headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/visits", "QUERY_STRING" => "", "rack.input" => StringIO.new(""), "HTTP_COOKIE" => cookie)
  raise "GET /visits answered #{status}" unless status == 200
  lines = Array(headers["set-cookie"]).flat_map { |line| line.split("\n") }
  text = +""
  body.each { |part| text << part }
  answer << "#{text}|#{lines.map { |line| line.split("=").first }.sort.join(",")}\n"
  cookie = lines.map { |line| line.split(";").first }.join("; ")
end
print answer
"##;

#[test]
fn rotated_cookies_read_and_go_out_under_the_current_secret() {
    let serve = "require \"rack\"\nRack::Builder.new.instance_eval(File.read(\"config.ru\"), \"config.ru\")\n";
    let run = app().run_ruby(&format!("{MINT}{serve}{REQUESTS}"));
    run.assert_passes();
    assert!(run.stdout.ends_with(EXPECTED), "{}", run.stdout);
}

/// The same cookies on the Spinel binary: minted by CRuby for a fixed secret, read through the controller's jar, and the
/// values it writes back read by a fresh jar.
#[test]
#[ignore = "requires the Spinel toolchain"]
fn rotated_cookies_read_and_go_out_under_the_current_secret_on_spinel() {
    let output = emit_and_run::ruby()
        .args(["-e", &format!("{MINT}print mint({SECRET:?}).join(\"\\n\")")])
        .output()
        .expect("spawn ruby");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let minted = String::from_utf8_lossy(&output.stdout).into_owned();
    let (signed, encrypted) = minted.split_once('\n').expect("two cookies");
    app()
        .run_spinel(&format!(
            r##"Rails.secret_key_base = {SECRET:?}
AppInitializers.run(false)
old = {{ "remember" => {signed:?}, "token" => {encrypted:?} }}
controller = VisitsController.new
controller.request_method = "GET"
controller.params = {{}}
controller.cookies = ActionController::CookieJar.new(old)
controller.process_action(:index)
raise "rotated read: #{{controller.body}}" unless controller.body == "old-signed|old-encrypted"
jar = controller.cookies
fresh = {{ "remember" => jar.raw("remember"), "token" => jar.raw("token") }}
raise "not written back under the current secret" if fresh["remember"] == old["remember"] || fresh["token"] == old["token"]
again = VisitsController.new
again.request_method = "GET"
again.params = {{}}
again.cookies = ActionController::CookieJar.new(fresh)
again.process_action(:index)
raise "current read: #{{again.body}}" unless again.body == "old-signed|old-encrypted"
raise "a current cookie is written again" unless again.cookies.raw("remember") == fresh["remember"]
puts "cookie rotation passed"
"##
        ))
        .assert_passes();
}

#[test]
fn rotated_cookies_read_as_rails_reads_them() {
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/rails_oracle.rb");
    let script = format!(
        r##"require {helper:?}
unless RailsOracle.available?("railties", "actionpack")
  warn "skipping cookie rotation oracle: #{{RailsOracle.label}} needs railties and actionpack"
  exit 77
end
RailsOracle.activate("railties", "actionpack")
require "rails"
require "action_controller/railtie"
{MINT}class RotationOracle < Rails::Application
  config.load_defaults 8.1
  config.eager_load = false
  config.hosts.clear
  config.logger = Logger.new(File::NULL)
  config.secret_key_base = {SECRET:?}
  routes.append {{ get "/visits" => "visits#index" }}
end
class ApplicationController < ActionController::Base
end
{CONTROLLER}{ROTATOR}RotationOracle.initialize!
module Main
  def self.run_rack(env)
    RotationOracle.call(Rack::MockRequest.env_for(env["PATH_INFO"], env))
  end
end
{REQUESTS}"##
    );
    let output = std::process::Command::new("ruby").args(["-e", &script]).output().expect("Rails oracle");
    if output.status.code() == Some(77) {
        prerequisite::missing(String::from_utf8_lossy(&output.stderr).trim());
        return;
    }
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED);
}

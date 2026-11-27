#![allow(dead_code)]

use super::emit_and_run;

pub fn app() -> emit_and_run::Overlay {
    with_flash(
        emit_and_run::empty_app()
            .write(
                "db/schema.rb",
                "ActiveRecord::Schema.define(version: 1) do\nend\n",
            )
            .write(
                "config/routes.rb",
                "Rails.application.routes.draw do\nend\n",
            ),
    )
}

pub fn with_flash(app: emit_and_run::Overlay) -> emit_and_run::Overlay {
    app.edit(
        "config/routes.rb",
        "Rails.application.routes.draw do\n",
        &format!("Rails.application.routes.draw do\n{ROUTES}"),
    )
    .write("app/controllers/messages_controller.rb", CONTROLLER)
}

const ROUTES: &str = r#"  get "/messages/create", to: "messages#create"
  get "/messages/temporary", to: "messages#temporary"
  get "/messages", to: "messages#show"
"#;

const CONTROLLER: &str = r#"class MessagesController < ActionController::Base
  def create
    flash[:notice] = "saved"
    flash[:inertia] = { "message" => "saved", "notification_id" => "notice-1", "tags" => ["one", 2, false, nil] }
    flash[:enabled] = false
    flash[:count] = 7
    flash[:empty] = []
    redirect_to "/messages"
  end

  def temporary
    flash.now[:inertia] = { "message" => "temporary" }
    render json: flash.to_h
  end

  def show
    render json: flash.to_h
  end
end
"#;

pub const RACK_ADAPTER: &str = r##"
require "json"
require "stringio"
def request_flash(path, jar)
  status, headers, body = Main.run_rack({ "REQUEST_METHOD" => "GET", "PATH_INFO" => path,
    "QUERY_STRING" => "", "HTTP_HOST" => "localhost", "rack.input" => StringIO.new(""),
    "HTTP_COOKIE" => jar.map { |key, value| "#{key}=#{value}" }.join("; ") })
  text = +""
  body.each { |part| text << part }
  body.close if body.respond_to?(:close)
  Array(headers["set-cookie"]).flat_map { |line| line.split("\n") }.each do |cookie|
    key, value = cookie.split(";", 2).first.split("=", 2)
    value.nil? || value.empty? ? jar.delete(key) : jar[key] = value
  end
  [status, status == 200 ? JSON.parse(text) : nil]
end
"##;

pub const ASSERTIONS: &str = r#"
jar = {}
status, = request_flash("/messages/create", jar)
raise "redirect status #{status}" unless status == 302
status, payload = request_flash("/messages", jar)
expected = { "notice" => "saved", "inertia" => { "message" => "saved", "notification_id" => "notice-1", "tags" => ["one", 2, false, nil] },
             "enabled" => false, "count" => 7, "empty" => [] }
raise "redirect lost flash: #{payload.inspect}" unless status == 200 && payload == expected
_, payload = request_flash("/messages", jar)
raise "flash repeated: #{payload.inspect}" unless payload == {}
_, payload = request_flash("/messages/temporary", jar)
raise "flash.now missing: #{payload.inspect}" unless payload == { "inertia" => { "message" => "temporary" } }
_, payload = request_flash("/messages", jar)
raise "flash.now persisted: #{payload.inspect}" unless payload == {}
request_flash("/messages/create", jar)
# The flash rides the encrypted session cookie: a tampered one reads as no session at all.
jar.each_key { |key| jar[key] = jar.fetch(key).sub(/.$/, "!") }
_, payload = request_flash("/messages", jar)
raise "tampered cookie accepted" if payload.key?("inertia")
puts "JSON flash survives one redirect and expires"
"#;

pub const CODEC_ASSERTIONS: &str = r#"
name = "_flash_codec_session"
values = ["saved", "123", "false", '{"message":"text"}', "✓",
          { "message" => "saved", "notification_id" => "notice-1" }, ["one", 2, false, nil], false, true, 7, 1.25, {}, []]
values.each do |value|
  session = ActionDispatch::Session.new
  session["flash"] = { "discard" => [], "flashes" => { "inertia" => value } }
  raw = ActionDispatch::CookieStore.cookie_value(session, name)
  got = ActionDispatch::CookieStore.inbound_flash(ActionDispatch::CookieStore.restore(raw, name))
  raise "flash value changed: #{value.inspect} -> #{got.inspect}" unless got == { "inertia" => value }
  raise "another cookie name accepted" unless ActionDispatch::CookieStore.restore(raw, name + "_other").to_hash.empty?
end
puts "String cookie bytes and JSON cookie purposes verified"
"#;

pub const HTTP_ADAPTER: &str = r#"
require "net/http"
module Main; end
def Main.run_rack(env)
  request = Net::HTTP::Get.new(env.fetch("PATH_INFO"))
  request["Cookie"] = env.fetch("HTTP_COOKIE", "")
  http = Net::HTTP.new("127.0.0.1", Integer(ENV.fetch("FLASH_PORT")), nil)
  http.open_timeout = 2
  http.read_timeout = 15
  response = http.request(request)
  headers = response.each_header.to_h
  headers["set-cookie"] = response.get_fields("set-cookie") || []
  [response.code.to_i, headers, [response.body.to_s]]
end
"#;

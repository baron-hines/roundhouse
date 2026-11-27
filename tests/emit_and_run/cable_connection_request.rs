#[test]
fn cable_connect_reads_the_handshake_request() {
    let run = super::emit_and_run::real_blog()
        .write(
            "app/channels/application_cable/connection.rb",
            "module ApplicationCable\n  class Connection < ActionCable::Connection::Base\n    def connect\n      raise 'path' unless request.path == '/cable'\n      raise 'listener' unless request.env['roundhouse.listener'] == 'unix'\n      raise 'header' unless request.headers['X-Probe'] == 'present'\n      raise 'params' unless request.params['token'] == 'probe'\n      raise 'remote_ip' unless request.remote_ip == '192.0.2.7'\n    end\n  end\nend\n",
        )
        .run_ruby(
            r#"%w[nio websocket/driver].each { |feature| $LOADED_FEATURES << (feature + ".rb") }
module NIO; end
module WebSocket; end
require_relative "cable"
env = { "REQUEST_METHOD" => "GET", "PATH_INFO" => "/cable",
        "QUERY_STRING" => "token=probe", "REMOTE_ADDR" => "192.0.2.7",
        "HTTP_X_PROBE" => "present", "roundhouse.listener" => "unix" }
connection = Cable.identify(env)
raise "request not retained" unless connection.request.equal?(connection.request)
raise "env not retained" unless connection.request.env.equal?(env)
puts "connection request passed"
"#,
        );
    run.assert_passes();
    assert_eq!(run.stdout, "connection request passed\n", "{}", run.stderr);
}

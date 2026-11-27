use super::cable_transmit_contract as contract;

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn a_channel_transmits_to_its_connection_natively() {
    let script = format!(
        r#"class Tep::WebSocket::Driver
  def text(text)
    puts text
    0
  end
end
connection = Cable.build_connection(ActionController::CookieJar.new(Tep.str_hash))
socket = Cable::Socket.new
socket.ws = Tep::WebSocket::Driver.new(-1)
connection.transport = socket
identifier = {identifier:?}
channel = Cable.build_channel("ReplyChannel", connection, identifier)
channel.subscribed
channel.perform_action(JSON.parse("{{\"action\":\"reply\",\"value\":\"<b>&\\n\"}}"))
"#,
        identifier = contract::IDENTIFIER,
    );
    let run = contract::overlay().run_spinel(&script);
    run.assert_passes();
    assert_eq!(run.stdout, contract::EXPECTED, "stderr:\n{}", run.stderr);
}

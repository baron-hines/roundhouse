use super::cable_transmit_contract as contract;

#[test]
fn a_channel_transmits_to_its_connection() {
    let script = format!(
        r#"%w[nio websocket/driver].each {{ |feature| $LOADED_FEATURES << (feature + ".rb") }}
module NIO; end
module WebSocket; end
require_relative "cable"
module Cable::Reactor
  @pending = []
  def self.post(&block)
    @pending << block
  end
  def self.drain
    @pending.shift.call until @pending.empty?
  end
end
connection = Cable::Connection.allocate
def connection.send_text(text)
  frames << text
end
def connection.frames
  @frames ||= []
end
identity = ActionCable::Connection::Base.new(nil)
identity.transport = connection
identifier = {identifier:?}
channel = Cable::Dispatch.subscribe(ReplyChannel, identity, identifier, JSON.parse(identifier)).channel
raise "subscription rejected" if channel.nil?
raise "worker wrote to socket" unless connection.frames.empty?
Cable::Reactor.drain
Cable::Dispatch.perform(channel, {{ "action" => "reply", "value" => "<b>&\n" }})
raise "action wrote to socket" unless connection.frames.size == 1
Cable::Reactor.drain
connection.frames.each {{ |frame| puts frame }}
"#,
        identifier = contract::IDENTIFIER,
    );
    let run = contract::overlay().run_ruby(&script);
    run.assert_passes();
    assert_eq!(run.stdout, contract::EXPECTED, "stderr:\n{}", run.stderr);
}

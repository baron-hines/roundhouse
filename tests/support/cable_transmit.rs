pub fn overlay() -> super::emit_and_run::Overlay {
    super::emit_and_run::real_blog()
        .write(
            "app/channels/application_cable/channel.rb",
            "module ApplicationCable\n  class Channel < ActionCable::Channel::Base\n  end\nend\n",
        )
        .write(
            "app/channels/reply_channel.rb",
            r#"class ReplyChannel < ApplicationCable::Channel
  def subscribed
    transmit({ type: "ready", enabled: false, missing: nil })
  end

  def reply(data)
    transmit({ type: "reply", value: data["value"], count: 7 })
  end
end
"#,
        )
}

pub const IDENTIFIER: &str = r#"{ "channel": "ReplyChannel", "tag": "a\"b" }"#;

pub const EXPECTED: &str = r#"{"identifier":"{ \"channel\": \"ReplyChannel\", \"tag\": \"a\\\"b\" }","message":{"type":"ready","enabled":false,"missing":null}}
{"identifier":"{ \"channel\": \"ReplyChannel\", \"tag\": \"a\\\"b\" }","message":{"type":"reply","value":"\u003cb\u003e\u0026\n","count":7}}
"#;

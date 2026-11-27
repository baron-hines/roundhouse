//! A `send` of a literal name on a receiver of unknown class (fugit's `TimeCursor`, unrolled from
//! `define_method(k) { @t.send(k) }`): the Spinel tree resolved `send` to `Faye::WebSocket#send`, a class's own
//! `send`, and refused `wday_in_month` as returning a bool. The send is written `__send__`, Kernel's alone, which is
//! the method Ruby's `send` is for such a receiver; a typed receiver keeps `send`, and so does a String argument (a socket's own `send(data)`).

use super::emit_and_run;

const PROBE: &str = r#"class Probe
  def initialize(raw)
    @raw = raw
  end

  def shout
    JSON.parse(@raw)["word"].send(:upcase)
  end

  def typed
    "quiet".send(:upcase)
  end

  def named
    JSON.parse(@raw)["word"].send("capitalize")
  end
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  get \"/probe\", to: \"probes#index\"\n")
        .write("lib/probe.rb", PROBE)
        .write(
            "app/controllers/probes_controller.rb",
            "class ProbesController < ApplicationController\n  def index\n    probe = Probe.new('{\"word\":\"hey\"}')\n    render plain: \"#{probe.shout} #{probe.typed} #{probe.named}\"\n  end\nend\n",
        )
}

#[test]
fn a_literal_send_on_an_unknown_receiver_is_kernel_s() {
    let run = app().run_ruby(
        r##"status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/probe", "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
raise "GET /probe answered #{status}" unless status == 200
text = +""
body.each { |part| text << part }
raise "probe: #{text.inspect}" unless text == "HEY QUIET Hey"
puts "literal send passed"
"##,
    );
    run.assert_passes();
    let probe = std::fs::read_to_string(run.emitted.join("app/models/probe.rb")).expect("emitted probe");
    assert!(probe.contains(r#"["word"].__send__(:upcase)"#), "{probe}");
    assert!(probe.contains(r#""quiet".send(:upcase)"#), "{probe}");
    assert!(probe.contains(r#"["word"].send("capitalize")"#), "{probe}");
}

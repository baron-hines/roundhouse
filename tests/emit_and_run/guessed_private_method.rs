//! A call on a receiver of unknown class (`rows.take(1)`) is read as a call of each app class that defines the method,
//! so its arguments widen that method's parameter. A private method is no such call's target: Ruby never reaches one
//! through a receiver. MissionControl's private `offset(executions_or_jobs)` took the Integer of
//! `executions_or_jobs.offset(offset_value)`, and the class's own `take` here took `1`, which the emitted RBS (what
//! Spinel trusts) declared as `take: ((Integer | ActiveRecord::Relation) source)`.

use super::emit_and_run;

const PROBE: &str = r#"class Probe
  def initialize(raw)
    @raw = raw
  end

  def counted
    take(Article.all).count
  end

  def taken
    JSON.parse(@raw).take(1).first.upcase
  end

  private

  def take(source)
    source
  end
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  get \"/probe\", to: \"probes#index\"\n")
        .write("lib/probe.rb", PROBE)
        .write(
            "app/controllers/probes_controller.rb",
            "class ProbesController < ApplicationController\n  def index\n    probe = Probe.new('[\"hey\"]')\n    render plain: \"#{probe.counted} #{probe.taken}\"\n  end\nend\n",
        )
}

const SEED: &str = r#"Article.create!(title: "First", body: "An article body long enough")
"#;

const EXPECTED: &str = "1 HEY";

#[test]
fn a_guessed_call_reaches_no_private_method() {
    let run = app().run_ruby(&format!(
            r##"{SEED}status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/probe", "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
raise "GET /probe answered #{{status}}" unless status == 200
text = +""
body.each {{ |part| text << part }}
raise "probe: #{{text.inspect}}" unless text == {EXPECTED:?}
puts "guessed private method passed"
"##
        ));
    run.assert_passes();
    let sig = std::fs::read_to_string(run.emitted.join("sig/app/models/probe.rbs")).expect("emitted probe signature");
    assert!(sig.contains("def take: (ActiveRecord::Relation source) -> ActiveRecord::Relation"), "{sig}");
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_guessed_call_reaches_no_private_method_on_spinel() {
    app()
        .run_spinel_with_db(&format!(
            r##"{SEED}controller = ProbesController.new
controller.request_method = "GET"
controller.params = {{}}
controller.process_action(:index)
raise "probe: #{{controller.body}}" unless controller.body == {EXPECTED:?}
puts "guessed private method passed"
"##
        ))
        .assert_passes();
}

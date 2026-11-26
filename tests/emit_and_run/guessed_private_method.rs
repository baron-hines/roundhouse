//! A helper called with a typed argument at one site and an untyped one at another (MissionControl's
//! `limit(executions)`, given a `safe_constantize`d execution relation and `SolidQueue::Job.finished`). Two things
//! typed it as the typed site's class alone, and the Ruby emit refused the untyped site's result
//! (`Article#upcase` is called, but the emitted Ruby defines no such method):
//! - the parameter was pinned `untyped`, but the return harvested before the pin was kept;
//! - a call on a receiver of unknown class (`rows.take(1)`) was read as a call of the class's private `take`, a
//!   method no call with a receiver reaches (MissionControl's `executions_or_jobs.offset(offset_value)`).

use super::emit_and_run;

const PROBE: &str = r#"class Probe
  def initialize(raw)
    @raw = raw
  end

  def typed
    pick(Article.all).count
  end

  def loose
    pick(JSON.parse(@raw)).first.upcase
  end

  def counted
    take(Article.all).count
  end

  def taken
    JSON.parse(@raw).take(1).first.upcase
  end

  private

  def pick(source)
    source
  end

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
            "class ProbesController < ApplicationController\n  def index\n    probe = Probe.new('[\"hey\"]')\n    render plain: \"#{probe.typed} #{probe.loose} #{probe.counted} #{probe.taken}\"\n  end\nend\n",
        )
}

const SEED: &str = r#"Article.create!(title: "First", body: "An article body long enough")
"#;

const EXPECTED: &str = "1 HEY 1 HEY";

#[test]
fn an_untyped_call_site_keeps_the_parameter_untyped() {
    let run = app().run_ruby(&format!(
            r##"{SEED}status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/probe", "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
raise "GET /probe answered #{{status}}" unless status == 200
text = +""
body.each {{ |part| text << part }}
raise "probe: #{{text.inspect}}" unless text == {EXPECTED:?}
puts "untyped call site passed"
"##
        ));
    run.assert_passes();
    let sig = std::fs::read_to_string(run.emitted.join("sig/app/models/probe.rbs")).expect("emitted probe signature");
    assert!(sig.contains("def pick: (untyped source) -> untyped"), "{sig}");
    assert!(sig.contains("def take: (ActiveRecord::Relation source) -> ActiveRecord::Relation"), "{sig}");
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_untyped_call_site_keeps_the_parameter_untyped_on_spinel() {
    app()
        .run_spinel_with_db(&format!(
            r##"{SEED}controller = ProbesController.new
controller.request_method = "GET"
controller.params = {{}}
controller.process_action(:index)
raise "probe: #{{controller.body}}" unless controller.body == {EXPECTED:?}
puts "untyped call site passed"
"##
        ))
        .assert_passes();
}

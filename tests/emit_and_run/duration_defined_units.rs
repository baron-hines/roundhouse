//! A duration unit a class also defines as its own reader (et-orbi's `EoTime#seconds`, which fugit's
//! `TimeCursor` reads as `@t.seconds = @t.seconds.to_i`): on a receiver not known to be a number, Ruby answers the
//! class's reader for an instance of it and ActiveSupport's Duration for a number. The unit used to become
//! `ActiveSupport::Duration.seconds(@t)` whatever `@t` held. A receiver that runs something (`current.seconds`) is
//! bound once and read by both arms, and a comparison or a Time sum over it still grounds the Duration arm.

use super::emit_and_run;

const LAP: &str = r#"class Lap
  def initialize(seconds)
    @seconds = seconds
  end

  def seconds
    @seconds
  end
end
"#;

const TIMER: &str = r##"class Timer
  def initialize(t)
    @t = t
  end

  def total
    @t.seconds.to_i
  end

  def through_a_call
    stored = current.seconds
    stored.to_i
  end

  def compared
    base = Time.at(0).utc
    "#{(base + current.seconds).to_i} #{current.seconds < 6}"
  end

  def past_base?
    base = Time.at(0).utc
    base > current.seconds
  end

  def current
    @t
  end
end
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("config/routes.rb", "  root \"articles#index\"\n", "  root \"articles#index\"\n  get \"/timer\", to: \"timers#index\"\n")
        .write("lib/lap.rb", LAP)
        .write("lib/timer.rb", TIMER)
        .write(
            "app/controllers/timers_controller.rb",
            "class TimersController < ApplicationController\n  def index\n    render plain: \"#{Timer.new(Lap.new(5)).total} #{Timer.new(3).total} #{Timer.new(Lap.new(7)).through_a_call} #{Timer.new(4).through_a_call} #{Timer.new(Lap.new(7)).compared} #{Timer.new(4).compared} #{Timer.new(4).past_base?}\"\n  end\nend\n",
        )
}

const EXPECTED: &str = "5 3 7 4 7 false 4 true true";

#[test]
fn a_unit_a_class_defines_answers_by_the_receiver_s_class() {
    app()
        .run_ruby(&format!(
            r##"status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/timer", "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
raise "GET /timer answered #{{status}}" unless status == 200
text = +""
body.each {{ |part| text << part }}
raise "timer: #{{text.inspect}}" unless text == {EXPECTED:?}
puts "duration defined units passed"
"##
        ))
        .assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_unit_a_class_defines_answers_by_the_receiver_s_class_on_spinel() {
    app()
        .run_spinel(&format!(
            r##"controller = TimersController.new
controller.request_method = "GET"
controller.params = {{}}
controller.process_action(:index)
raise "timer: #{{controller.body}}" unless controller.body == {EXPECTED:?}
puts "duration defined units passed"
"##
        ))
        .assert_passes();
}

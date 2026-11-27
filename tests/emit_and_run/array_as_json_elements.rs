//! An Array's (or a Hash's) `as_json` maps each element of an app class
//! through the element's own `as_json`, whether its class writes it, its
//! superclass does, or a module its superclass includes does, as
//! ActiveSupport's `as_json` asks each value. A direct call of an
//! inherited `as_json` already answers it here. Ignored as a fork spec
//! until the compiler does; the expected values are ActiveRecord 8.1.4's
//! answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/shape.rb", r##"class Shape
  def initialize(sides)
    @sides = sides
  end

  def as_json(options = nil)
    { "sides" => @sides }
  end
end
"##)
        .write("app/models/square.rb", r##"class Square < Shape
  def area
    4
  end
end
"##)
        .write("app/models/labelled.rb", r##"module Labelled
  def as_json(options = nil)
    { "label" => label }
  end
end
"##)
        .write("app/models/marker.rb", r##"class Marker
  include Labelled

  def label
    "plain"
  end
end
"##)
        .write("app/models/flag_marker.rb", r##"class FlagMarker < Marker
  def label
    "flag"
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    # Rails: [{"sides"=>5}, {"sides"=>3}, {"label"=>"flag"}]
    list = [Shape.new(5), Square.new(3), FlagMarker.new].as_json
    check("array elements", list == [{ "sides" => 5 }, { "sides" => 3 }, { "label" => "flag" }])
    # Rails: {"shapes"=>[{"sides"=>4}]}
    check("hash values", { "shapes" => [Square.new(4)] }.as_json == { "shapes" => [{ "sides" => 4 }] })
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: ruby semantics"]
fn an_array_as_json_maps_each_element_through_its_own_as_json() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn an_array_as_json_maps_each_element_through_its_own_as_json_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

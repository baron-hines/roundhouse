//! svg-graph's `DataPoint`, in the three spellings its class body uses
//! that ingest refused or dropped: a class method defined on the class's
//! own name (`def DataPoint.configure_shape_criteria`), constants written
//! `unless defined?` themselves, and a `lambda { … }` constant. The whole
//! file was refused, so `Plot#draw_data` (lobsters' `/stats`) raised on an
//! unsupported constant.

use super::emit_and_run;

const SHAPES: &str = r##"class Shape
  OVERLAY = "OVERLAY"
  DEFAULT = lambda { |x, y| ["circle", { "cx" => x, "cy" => y }] } unless defined? DEFAULT
  CRITERIA = [] unless defined? CRITERIA

  def Shape.configure(*matchers)
    CRITERIA.push(*matchers)
  end

  def initialize(x, y)
    @x = x
    @y = y
  end

  def render
    shapes = CRITERIA.select { |c| c.size == 2 }.map { |_, made| made.call(@x, @y) }
    shapes = [DEFAULT.call(@x, @y)] if shapes.empty?
    shapes
  end
end
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/shape.rb", SHAPES)
}

const ASSERTIONS: &str = r##"first = Shape.new(1, 2).render
Shape.configure([/x/, lambda { |x, y| ["rect", { "x" => x, "y" => y }] }])
second = Shape.new(3, 4).render
got = [first, second, Shape::OVERLAY]
raise got.inspect unless got == [[["circle", { "cx" => 1, "cy" => 2 }]], [["rect", { "x" => 3, "y" => 4 }]], "OVERLAY"]
puts "data point shapes passed"
"##;

#[test]
fn data_point_shapes_run() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("data point shapes passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn data_point_shapes_run_on_spinel() {
    let run = app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("data point shapes passed"), "{}", run.stdout);
}

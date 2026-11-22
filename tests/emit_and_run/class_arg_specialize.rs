//! A controller method handed a class, copied per class
//! (`ingest::class_arg_specialize`): each call reaches the method and the
//! class Ruby would, whatever the copy is named.

use super::emit_and_run;

const CLASSES: &[(&str, &str)] = &[
    ("app/models/alpha.rb", "class Alpha\n  def label\n    \"alpha\"\n  end\nend\n"),
    ("app/models/beta.rb", "class Beta\n  def label\n    \"beta\"\n  end\nend\n"),
    ("app/models/ns.rb", "module Ns\n  class Gamma\n    def label\n      \"ns gamma\"\n    end\n  end\nend\n"),
    ("app/models/ns_gamma.rb", "class Ns_Gamma\n  def label\n    \"flat gamma\"\n  end\nend\n"),
];

const BASE: &str = r##"class SpecBaseController < ApplicationController
  def run
    render plain: build(Alpha).label
  end

  def splat
    prefix = [0, Beta]
    render plain: pick(*prefix, Alpha).label
  end

  def pairs
    x = Beta
    render plain: [pair(Alpha, x), pair(x, Alpha)].join(",")
  end

  def gammas
    render plain: [build(Ns::Gamma).label, build(Ns_Gamma).label].join(",")
  end

  def defaulted
    render plain: wrap(Alpha)
  end

  def referenced
    render plain: ([make_one(Alpha).label] + [Beta].map(&method(:make_one)).map(&:label)).join(",")
  end

  def shadowed
    render plain: make_two(Alpha)
  end

  def supered
    render plain: build_super(Alpha).label
  end

  private

  def build(k)
    k.new
  end

  def pick(first, klass, *rest)
    klass.new
  end

  def pair(left, right)
    "#{left.new.label} #{right.new.label}"
  end

  def make_two(klass)
    [->(klass = Beta) { klass.new.label }.call, klass.new.label].join(" ")
  end

  def make_one(k)
    k.new
  end

  def build_super(klass)
    klass.new
  end

  def wrap(klass, opts = { made: klass })
    "#{klass.new.label} #{opts[:made].name}"
  end
end
"##;

const CHILD: &str = r##"class SpecChildController < SpecBaseController
  def supered
    render plain: build_super(Alpha).label
  end

  private

  def build_super(klass)
    made = super
    klass.new.label == made.label ? made : nil
  end

  def build(k)
    Beta.new
  end
end
"##;

const OTHER: &str = r##"class SpecOtherController < ApplicationController
  def run
    render plain: "#{build(Alpha).name} #{make(Alpha).label}"
  end

  private

  def build(k)
    k
  end

  def make(k)
    k.new
  end
end
"##;

fn overlay() -> emit_and_run::Overlay {
    let mut o = emit_and_run::real_blog()
        .write("app/controllers/spec_base_controller.rb", BASE)
        .write("app/controllers/spec_child_controller.rb", CHILD)
        .write("app/controllers/spec_other_controller.rb", OTHER)
        .edit(
            "config/routes.rb",
            "  resources :articles do",
            &(["run", "splat", "pairs", "gammas", "defaulted", "referenced", "shadowed", "supered"].map(|a| format!("  get \"spec_base/{a}\" => \"spec_base#{a}\"\n")).concat()
                + "  get \"spec_child/run\" => \"spec_child#run\"\n  get \"spec_child/supered\" => \"spec_child#supered\"\n  get \"spec_other/run\" => \"spec_other#run\"\n  resources :articles do"),
        );
    for (path, src) in CLASSES {
        o = o.write(path, src);
    }
    o
}

#[test]
fn a_class_handed_to_a_controller_method_reaches_what_ruby_calls() {
    let script = r##"
require_relative "app/controllers/spec_base_controller"
require_relative "app/controllers/spec_child_controller"
require_relative "app/controllers/spec_other_controller"
run = ->(klass, action) { c = klass.new; c.params = {}; c.process_action(action); c.body }
{
  [SpecBaseController, :run] => "alpha",
  [SpecChildController, :run] => "beta",
  [SpecBaseController, :splat] => "beta",
  [SpecBaseController, :pairs] => "alpha beta,beta alpha",
  [SpecBaseController, :gammas] => "ns gamma,flat gamma",
  [SpecBaseController, :defaulted] => "alpha Alpha",
  [SpecOtherController, :run] => "Alpha alpha",
  [SpecBaseController, :referenced] => "alpha,beta",
  [SpecBaseController, :shadowed] => "beta alpha",
  [SpecChildController, :supered] => "alpha",
}.filter_map do |(klass, action), want|
  got = begin; run.(klass, action); rescue StandardError => e; e.class.name; end
  "#{klass}##{action}: #{got.inspect}" unless got == want
end.then { |bad| raise bad.join("; ") unless bad.empty? }
puts "class arg passed"
"##;
    let run = overlay().run_ruby(script);
    run.assert_passes();
    assert!(run.stdout.contains("class arg passed"));
}

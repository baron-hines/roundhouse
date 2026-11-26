use roundhouse::build_target::BuildTarget;

use super::emit_and_run;

const GEMS: &str = "vendor/bundle/ruby/3.4.0/gems";

/// An installed svg-graph (a stand-in: one class), compiled from source.
fn with_svg_gem() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    svg-graph (9.9)\n    debug (1.11.1)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  svg-graph\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write(&format!("{GEMS}/svg-graph-9.9/lib/svg/graph.rb"), "require \"svg/graph/time_series\"\n")
        .write(
            &format!("{GEMS}/svg-graph-9.9/lib/svg/graph/time_series.rb"),
            "module SVG\n  module Graph\n    class TimeSeries\n      def get_x_values\n        [1]\n      end\n    end\n  end\nend\n",
        )
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  def x_values\n    SVG::Graph::TimeSeries.new.get_x_values\n  end\n\n",
        )
}

/// A gem compiled from source that defines `SVG` (svg-graph) leaves out
/// the raising `SVG::Graph::TimeSeries` façade: two definitions of one
/// method are what spinel refuses.
#[test]
fn a_namespace_a_source_compiled_gem_defines_leaves_the_gem_facade() {
    let (emitted, _errors) = with_svg_gem().emit(BuildTarget::Spinel);
    let facades = std::fs::read_to_string(emitted.join("runtime/gem_facades.rb")).expect("gem_facades.rb");
    assert!(!facades.contains("class TimeSeries"), "{facades}");
    // Not the namespaces: the gem's compound `class SVG::Graph::…` headers look them up, and svg-graph declares nothing in them.
    assert!(facades.contains("module SVG\n  module Graph\n  end\nend\n"), "{facades}");
    assert!(facades.contains("module Markly"), "the other façades stay:\n{facades}");
    let sig = std::fs::read_to_string(emitted.join("runtime/gem_facades.rbs")).expect("gem_facades.rbs");
    assert!(!sig.contains("class TimeSeries"), "{sig}");
    assert!(sig.contains("module SVG\n  module Graph\n  end\nend\n"), "{sig}");
}

/// An app's own reopening of a façade's namespace (campfire's
/// `module WebPush`) keeps the façade: the app does not define what it fills.
#[test]
fn an_app_file_in_the_namespace_keeps_the_facade() {
    let app = emit_and_run::real_blog().write(
        "app/models/svg/graph/time_series_ext.rb",
        "module SVG\n  module Graph\n    class Helper\n      def self.label\n        \"x\"\n      end\n    end\n  end\nend\n",
    );
    let (emitted, _errors) = app.emit(BuildTarget::Spinel);
    let facades = std::fs::read_to_string(emitted.join("runtime/gem_facades.rb")).expect("gem_facades.rb");
    assert!(facades.contains("module SVG"), "{facades}");
}

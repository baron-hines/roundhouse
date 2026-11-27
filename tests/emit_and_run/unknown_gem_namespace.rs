use super::emit_and_run;

/// An unknown gem whose namespace is not its name camelized: svg-graph
/// defines `SVG::Graph`. Nothing declared it, so lobsters' CRuby tree
/// stopped at boot on `class TimeSeries < SVG::Graph::TimeSeries`. The
/// bundled gem the app's own Gemfile lists beside it (`rexml`, which
/// svg-graph loads without declaring) goes in too.
#[test]
fn an_unknown_gem_owning_a_differently_cased_namespace_is_declared() {
    let (emitted, errors) = emit_and_run::real_blog()
        .edit("Gemfile.lock", "  specs:\n", "  specs:\n    rexml (3.4.1)\n    svg-graph (2.2.2)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  rexml\n  svg-graph\n")
        .write("app/models/time_series.rb", "class TimeSeries < SVG::Graph::TimeSeries\nend\n")
        .edit("Gemfile", "gem \"rails\", \"~> 8.1.4\"\n", "gem \"rails\", \"~> 8.1.4\"\ngem \"svg-graph\", require: \"SVG/Graph/TimeSeries\"\n")
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let gemfile = std::fs::read_to_string(emitted.join("Gemfile")).expect("emitted Gemfile");
    assert!(gemfile.contains("gem \"svg-graph\", \"= 2.2.2\""), "{gemfile}");
    assert!(gemfile.contains("gem \"rexml\""), "{gemfile}");
    // What the app's Gemfile says Bundler requires: svg-graph has no `svg-graph.rb`.
    let boot = std::fs::read_to_string(emitted.join("boot.rb")).expect("emitted boot.rb");
    assert!(boot.contains("require \"SVG/Graph/TimeSeries\""), "{boot}");
}

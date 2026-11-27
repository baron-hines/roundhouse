use super::emit_and_run;

/// A class or reference of an unknown gem (in the lockfile, with no façade
/// and no spin package): the CRuby tree requires the real gem, but nothing
/// in the Spinel tree defines it. lobsters' `SearchParser < Parslet::Parser`
/// compiled clean and stopped the native server at boot with a
/// NoMethodError for `rule`. The Spinel emit now reports each use; a
/// constant a carried gem owns (`Rack::Utils`) is not one.
fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "  specs:\n", "  specs:\n    parslet (2.0.0)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  parslet\n")
        .write(
            "app/models/search_parser.rb",
            "require \"parslet\"\n\nclass SearchParser < Parslet::Parser\n  rule(:space) { match('\\s').repeat(1) }\n  root(:space)\nend\n",
        )
        .write("app/models/escaper.rb", "class Escaper\n  def self.call(text)\n    Rack::Utils.escape_html(text)\n  end\nend\n")
}

#[test]
fn spinel_reports_an_unknown_gem_s_constants() {
    let (_emitted, errors) = app().emit(roundhouse::project::BuildTarget::Spinel);
    let found: Vec<&String> = errors.iter().filter(|e| e.contains("constant of an unknown gem not supported (spinel)")).collect();
    assert!(found.iter().any(|e| e.contains("`Parslet::Parser` belongs to the parslet gem")), "{errors:#?}");
    assert!(!errors.iter().any(|e| e.contains("Rack::Utils")), "{errors:#?}");
    let (_emitted, errors) = app().emit(roundhouse::project::BuildTarget::Ruby);
    assert!(!errors.iter().any(|e| e.contains("unknown gem")), "{errors:#?}");
}

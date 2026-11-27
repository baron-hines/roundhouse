use super::emit_and_run;

/// `include Rails.application.routes.url_helpers` in an app class, in its
/// singleton class (lobsters' Routes, with its own `default_url_options`)
/// and on instances (forem's mailers, chatwoot's services). Ingest records
/// it as an include of the generated RouteHelpers, which the include check
/// refused as unresolved. A class's own `default_url_options` is the host
/// Rails' `_url` helpers answer on there.
fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "app/models/routes.rb",
            "class Routes\n  class << self\n    include Rails.application.routes.url_helpers\n\n    def default_url_options\n      { host: \"example.com\" }\n    end\n\n    def title_path(article)\n      article_path(article)\n    end\n\n    def title_url(article)\n      article_url(article)\n    end\n  end\nend\n",
        )
        .write(
            "app/models/link_builder.rb",
            "class LinkBuilder\n  include Rails.application.routes.url_helpers\n\n  def initialize(article)\n    @article = article\n  end\n\n  def path\n    article_path(@article)\n  end\n\n  def url\n    article_url(@article, host: \"example.org\")\n  end\nend\n",
        )
        .write(
            "test/models/url_helpers_include_test.rb",
            "require \"test_helper\"\n\nclass UrlHelpersIncludeTest < ActiveSupport::TestCase\n  test \"on the singleton, with the class's own default_url_options\" do\n    article = articles(:one)\n    assert_equal \"/articles/#{article.id}\", Routes.title_path(article)\n    assert_equal \"http://example.com/articles/#{article.id}\", Routes.title_url(article)\n  end\n\n  test \"on instances\" do\n    article = articles(:one)\n    builder = LinkBuilder.new(article)\n    assert_equal \"/articles/#{article.id}\", builder.path\n    assert_equal \"http://example.org/articles/#{article.id}\", builder.url\n  end\nend\n",
        )
}

#[test]
fn an_included_url_helpers_surface_runs_on_the_ruby_family() {
    let (_emitted, errors) = app().emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    app().run_test("test/models/url_helpers_include_test.rb").assert_passes();
}

/// TypeScript carries the class but not the helpers it would inherit.
#[test]
fn a_strict_target_reports_the_include() {
    let (_emitted, errors) = app().emit(roundhouse::project::BuildTarget::Typescript);
    let found: Vec<&String> = errors.iter().filter(|e| e.contains("include of the route url_helpers not supported (typescript)")).collect();
    assert_eq!(found.len(), 2, "{errors:#?}");
}

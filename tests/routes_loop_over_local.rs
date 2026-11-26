//! A route loop over a local that holds a word list (`pages_actions =
//! %w[welcome editor_guide].freeze; pages_actions.each { … }`, forem's
//! `/p/` pages) draws one route per word, as a literal list does. It was
//! refused as a receiver that is not a literal Array, and the pages had no
//! route.

use roundhouse::App;
use roundhouse::ingest::ingest_routes;
use roundhouse::lower::routes::flatten_routes;

fn routes(source: &str) -> Vec<(String, String)> {
    let mut app = App::default();
    app.routes = ingest_routes(format!("Rails.application.routes.draw do\n{source}\nend\n").as_bytes(), "config/routes.rb")
        .expect("ingest routes");
    flatten_routes(&app).into_iter().map(|r| (r.path, format!("{}#{}", r.controller.0.as_str(), r.action.as_str()))).collect()
}

#[test]
fn a_loop_over_a_frozen_word_list_in_a_local_draws_each_route() {
    let drawn = routes(
        r#"scope "p" do
  pages_actions = %w[welcome editor_guide].freeze
  pages_actions.each do |action|
    get action, action: action, controller: "pages"
  end
end"#,
    );
    assert_eq!(
        drawn,
        [
            ("/p/welcome".to_string(), "PagesController#welcome".to_string()),
            ("/p/editor_guide".to_string(), "PagesController#editor_guide".to_string()),
        ],
        "{drawn:?}"
    );
}

#[test]
fn a_loop_over_a_local_that_is_no_word_list_is_still_refused() {
    let mut app = App::default();
    let err = ingest_routes(
        b"Rails.application.routes.draw do\n  pages = Page.all\n  pages.each do |page|\n    get page.slug, to: \"pages#show\"\n  end\nend\n",
        "config/routes.rb",
    )
    .map(|routes| app.routes = routes)
    .expect_err("a list ingest cannot read");
    assert!(err.to_string().contains("not a literal Array"), "{err}");
}

/// A call on the local before the loop (`pages_actions << "faq"`) may change the list, so the loop
/// is a gap rather than the routes of the list as first assigned.
#[test]
fn a_loop_over_a_list_a_call_may_have_changed_is_refused() {
    for change in [
        "pages_actions << \"faq\"",
        "pages_actions.concat(%w[faq])",
        "alias_list = pages_actions\n  alias_list.push(\"faq\")",
        "%w[once].each do |step|\n    pages_actions << \"faq\"\n  end",
    ] {
        let source = format!(
            "Rails.application.routes.draw do\n  pages_actions = %w[welcome].freeze\n  {change}\n  pages_actions.each do |action|\n    get action, controller: \"pages\", action: action\n  end\nend\n"
        );
        let err = ingest_routes(source.as_bytes(), "config/routes.rb").expect_err(&change);
        assert!(err.to_string().contains("not a literal Array"), "{change}: {err}");
    }
}

/// A list changed inside a drawn file (`draw :admin`) is that file's: the caller's own list, in a
/// frame the drawn file does not share, still draws its loop.
#[test]
fn a_list_changed_in_a_drawn_file_leaves_the_callers_list_alone() {
    let table = roundhouse::ingest::routes::ingest_routes_with_draws(
        b"Rails.application.routes.draw do\n  pages = %w[welcome]\n  draw :admin\n  pages.each do |page|\n    get page, controller: \"pages\", action: page\n  end\nend\n",
        "config/routes.rb",
        &roundhouse::det::HashMap::from_iter([(
            "admin".to_string(),
            (b"items = %w[a]\nitems << \"b\"\n".to_vec(), "config/routes/admin.rb".to_string()),
        )]),
    )
    .expect("ingest routes");
    let mut app = App::default();
    app.routes = table;
    let paths: Vec<String> = flatten_routes(&app).into_iter().map(|r| r.path).collect();
    assert_eq!(paths, ["/welcome"], "{paths:?}");
}

/// A loop body that assigns a local its scope already holds (`pages = %w[faq]`) changes the scope's
/// local, as in Ruby, so a later loop over it draws the new list.
#[test]
fn an_assignment_inside_a_loop_reaches_the_enclosing_local() {
    let drawn = routes(
        r#"pages = %w[welcome]
%w[once].each do |step|
  pages = %w[faq]
end
pages.each do |page|
  get page, controller: "pages", action: page
end"#,
    );
    assert_eq!(drawn, [("/faq".to_string(), "PagesController#faq".to_string())], "{drawn:?}");
}

/// The block parameter shadows a scope local of the same name, which the loop leaves as it was.
#[test]
fn a_block_parameter_does_not_overwrite_the_scope_local_it_shadows() {
    let drawn = routes(
        r#"page = "home"
%w[a b].each do |page|
  get page, controller: "pages", action: page
end
get page, controller: "pages", action: page"#,
    );
    let paths: Vec<&str> = drawn.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths, ["/a", "/b", "/home"], "{drawn:?}");
}

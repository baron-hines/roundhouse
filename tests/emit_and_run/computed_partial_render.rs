//! maybe's `render partial: permitted_import_configuration_path(@import)`: a
//! partial whose name an app helper computes as the page renders. Ingest turns
//! it into a `case` over the partials the helper's values can name, each arm
//! an ordinary render, so the chosen partial reads the caller's ivars and its
//! locals; a name outside them raises `ActionView::MissingTemplate`, as Rails'
//! lookup does. A name computed any other way stays reported.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "app/helpers/kinds_helper.rb",
            "module KindsHelper\n  def kind_partial(article)\n    if article.title.start_with?(\"x\")\n      raise \"unknown kind\"\n    else\n      \"articles/kinds/#{article.title.downcase}\"\n    end\n  end\nend\n",
        )
        .write("app/views/articles/kinds/_hello.html.erb", "<p>hello <%= @article.title %> <%= article.body %></p>\n")
        .write("app/views/articles/kinds/_bye.html.erb", "<p>bye <%= article.title %></p>\n")
        .edit(
            "app/views/articles/show.html.erb",
            "    <p class=\"text-gray-700\">",
            "<%= render partial: kind_partial(@article), locals: { article: @article } %>\n    <p class=\"text-gray-700\">",
        )
}

const ASSERTIONS: &str = r#"
require "stringio"
def page(id)
  env = { "REQUEST_METHOD" => "GET", "PATH_INFO" => "/articles/#{id}", "QUERY_STRING" => "", "HTTP_ACCEPT" => "text/html" }
  r = Main.dispatch_core(env, StringIO.new(""))
  [r[0], r[1].is_a?(Array) ? r[1].join : r[1].to_s]
end
hello = Article.create!(title: "Hello", body: "Body text here")
bye = Article.create!(title: "Bye", body: "Another body text")
other = Article.create!(title: "Other", body: "Third body text")
got = page(hello.id)
raise "hello #{got[0]} #{got[1][0, 300]}" unless got[0] == 200 && got[1].include?("<p>hello Hello Body text here</p>")
got = page(bye.id)
raise "bye #{got[0]}" unless got[0] == 200 && got[1].include?("<p>bye Bye</p>")
missing = begin
  page(other.id)[0]
rescue ActionView::MissingTemplate
  :missing
end
raise "missing #{missing.inspect}" unless missing == :missing || missing == 500
puts "computed partial passed"
"#;

#[test]
fn a_helper_computed_partial_renders_the_one_it_names() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("computed partial passed"), "{}", run.stdout);
}

/// The Spinel binary has no Rack dispatcher: the controller renders the page directly.
const SPINEL_ASSERTIONS: &str = r#"
def page(id)
  controller = ArticlesController.new
  controller.request_method = "GET"
  controller.params = { "id" => id.to_s }
  controller.process_action(:show)
  controller.body
end
hello = Article.create!(title: "Hello", body: "Body text here")
bye = Article.create!(title: "Bye", body: "Another body text")
raise page(hello.id)[0, 300] unless page(hello.id).include?("<p>hello Hello Body text here</p>")
raise "bye" unless page(bye.id).include?("<p>bye Bye</p>")
puts "computed partial passed"
"#;

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_helper_computed_partial_renders_the_one_it_names_on_spinel() {
    let run = app().run_spinel_with_db(SPINEL_ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("computed partial passed"), "{}", run.stdout);
}

#[test]
fn a_partial_name_computed_otherwise_is_reported() {
    let (_emitted, errors) = emit_and_run::real_blog()
        .edit(
            "app/views/articles/show.html.erb",
            "    <p class=\"text-gray-700\">",
            "<%= render partial: @article.title.downcase, locals: { article: @article } %>\n    <p class=\"text-gray-700\">",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("render of a computed partial")), "{errors:#?}");
}

//! `<% raise ArgumentError, "…" %>` in a template raises as Rails renders the
//! page. The view walker took it for an unknown statement and wrote
//! `io << ""` in its place, so the page rendered on past it.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit(
        "app/views/articles/show.html.erb",
        "    <p class=\"text-gray-700\">",
        "<% if @article.title == \"Zed\" %><% raise ArgumentError, \"no zed\" %><% end %>\n    <p class=\"text-gray-700\">",
    )
}

const SCRIPT: &str = r#"
def page(id)
  controller = ArticlesController.new
  controller.request_method = "GET"
  controller.params = { "id" => id.to_s }
  controller.process_action(:show)
  controller.body
end
fine = Article.create!(title: "Fine", body: "Body text here")
zed = Article.create!(title: "Zed", body: "Body text here")
raise "fine page" unless page(fine.id).include?("Body text here")
message = begin
  page(zed.id)
  "rendered"
rescue ArgumentError => e
  e.message
end
raise "zed #{message}" unless message == "no zed"
puts "view raise passed"
"#;

#[test]
fn a_template_raise_raises() {
    let run = app().run_ruby(&format!("require_relative \"app/controllers/articles_controller\"\n{SCRIPT}"));
    run.assert_passes();
    assert!(run.stdout.contains("view raise passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_template_raise_raises_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("view raise passed"), "{}", run.stdout);
}

//! maybe's `render "accounts/form", account: account, url: url do |form| …
//! end` over a partial that `<%= yield form %>`s: Rails runs the block at the
//! yield, with the yield's arguments, once per yield. Ingest gives the call a
//! copy of the partial with the block's body at each yield, the block's
//! parameters bound to what it yields, and the caller's locals passed in, so
//! the body reads the yielded value, the caller's locals and the page's ivars
//! where Rails does, also in a loop and in a strict-locals partial.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "app/views/articles/_frame.html.erb",
            "<section><%= title %>:<% [1, 2].each do |i| %>[<%= yield \"slot#{i}\", i %>]<% end %></section>\n",
        )
        .write("app/views/articles/_strict.html.erb", "<%# locals: (heading:) %>\n<aside><%= heading %>|<%= yield %>|</aside>\n")
        .edit(
            "app/views/articles/show.html.erb",
            "    <p class=\"text-gray-700\">",
            "<% note = \"n1\" %><%= render \"articles/frame\", title: \"T\" do |label, n| %><i><%= label %>-<%= n * 10 %>-<%= note %>-<%= @article.title %></i><% end %>\
<%= render layout: \"articles/strict\", locals: { heading: \"H\" } do %><b><%= note %></b><% end %>\n    <p class=\"text-gray-700\">",
        )
}

const WANT: &[&str] = &[
    "<section>T:[<i>slot1-10-n1-Hello</i>][<i>slot2-20-n1-Hello</i>]</section>",
    "<aside>H|<b>n1</b>|</aside>",
];

fn assertions(page: &str) -> String {
    let checks: String = WANT.iter().map(|w| format!("raise body[0, 2000] unless body.include?({w:?})\n")).collect();
    format!("{page}\nhello = Article.create!(title: \"Hello\", body: \"Body text here\")\nbody = page(hello.id)\n{checks}puts \"yield partial passed\"\n")
}

const RUBY_PAGE: &str = r#"
require "stringio"
def page(id)
  env = { "REQUEST_METHOD" => "GET", "PATH_INFO" => "/articles/#{id}", "QUERY_STRING" => "", "HTTP_ACCEPT" => "text/html" }
  r = Main.dispatch_core(env, StringIO.new(""))
  raise "status #{r[0]}" unless r[0] == 200
  r[1].is_a?(Array) ? r[1].join : r[1].to_s
end
"#;

/// The Spinel binary has no Rack dispatcher: the controller renders the page directly.
const SPINEL_PAGE: &str = r#"
def page(id)
  controller = ArticlesController.new
  controller.request_method = "GET"
  controller.params = { "id" => id.to_s }
  controller.process_action(:show)
  controller.body
end
"#;

#[test]
fn a_block_render_runs_the_body_at_each_yield() {
    let run = app().run_ruby(&assertions(RUBY_PAGE));
    run.assert_passes();
    assert!(run.stdout.contains("yield partial passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_block_render_runs_the_body_at_each_yield_on_spinel() {
    let run = app().run_spinel_with_db(&assertions(SPINEL_PAGE));
    run.assert_passes();
    assert!(run.stdout.contains("yield partial passed"), "{}", run.stdout);
}

/// A yield that is not an output, and a block that leaves early, stay reported.
#[test]
fn other_yields_stay_reported() {
    for (partial, call) in [
        ("<% got = yield 1 %><p><%= got %></p>\n", "<%= render \"articles/frame\" do |n| %>b<% end %>"),
        ("<p><%= yield 1 %></p>\n", "<%= render \"articles/frame\" do |n| %><% next if n == 1 %>b<% end %>"),
    ] {
        let (_emitted, errors) = emit_and_run::real_blog()
            .write("app/views/articles/_frame.html.erb", partial)
            .edit("app/views/articles/show.html.erb", "    <p class=\"text-gray-700\">", &format!("{call}\n    <p class=\"text-gray-700\">"))
            .emit(roundhouse::project::BuildTarget::Ruby);
        assert!(errors.iter().any(|e| e.contains("render with a block")), "{partial} {call}: {errors:#?}");
    }
}

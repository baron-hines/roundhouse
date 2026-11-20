//! maybe's `render "accounts/form", account: account, url: url do |form| …
//! end` over a partial that `<%= yield form %>`s: Rails runs the block at the
//! yield, with the yield's arguments, once per yield. Ingest gives the call a
//! copy of the partial with the block's body at each yield, the block's
//! parameters bound to what it yields, and the caller's locals passed in, so
//! the body reads the yielded value, the caller's locals and the page's ivars
//! where Rails does, also in a loop and in a strict-locals partial, each
//! argument taken when the yield evaluates it.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "app/views/articles/_frame.html.erb",
            "<section><%= title %>:<% [1, 2].each do |i| %>[<%= yield \"slot#{i}\", i %>]<% end %></section>\n",
        )
        .write("app/views/articles/_snap.html.erb", "<% n = 1 %><% change = -> { n = 3 } %><u><%= yield n, (n = 2) %>/<%= yield n, change.call %>/<%= n %></u>\n")
        .write("app/views/articles/_cap.html.erb", "<% [1].each do |n| %><s><%= yield n, n %></s><% end %>\n")
        .write("app/views/articles/_strict.html.erb", "<%# locals: (heading:) %>\n<aside><%= heading %>|<%= yield %>|</aside>\n")
        .edit(
            "app/views/articles/show.html.erb",
            "    <p class=\"text-gray-700\">",
            "<% note = \"n1\" %><%= render \"articles/frame\", title: \"T\" do |label, n| %><i><%= label %>-<%= n * 10 %>-<%= note %>-<%= @article.title %></i><% end %>\
<%= render \"articles/snap\" do |a, _b| %><%= a %><% end %>\
<%= render \"articles/cap\" do |a, b| %><%= [2].map { |x| b + x }.join %>-<%= a %><% end %>\
<%= render layout: \"articles/strict\", locals: { heading: \"H\" } do %><b><%= note %></b><% end %>\n    <p class=\"text-gray-700\">",
        )
}

const WANT: &[&str] = &[
    "<section>T:[<i>slot1-10-n1-Hello</i>][<i>slot2-20-n1-Hello</i>]</section>",
    "<u>1/2/3</u>",
    "<s>3-1</s>",
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
        ("<p><%= yield(*[1, 2]) %></p>\n", "<%= render \"articles/frame\" do |a, b| %><%= a %><% end %>"),
    ] {
        let (_emitted, errors) = emit_and_run::real_blog()
            .write("app/views/articles/_frame.html.erb", partial)
            .edit("app/views/articles/show.html.erb", "    <p class=\"text-gray-700\">", &format!("{call}\n    <p class=\"text-gray-700\">"))
            .emit(roundhouse::project::BuildTarget::Ruby);
        assert!(errors.iter().any(|e| e.contains("render with a block")), "{partial} {call}: {errors:#?}");
    }
}

/// A block render alone in a loop body, one inside another's block, and a
/// block writing the caller's local (left to the captured-string path,
/// which closes over it, as Rails' block does).
fn nested_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/views/articles/_box.html.erb", "<u><%= yield %></u>")
        .edit(
            "app/views/articles/show.html.erb",
            "    <p class=\"text-gray-700\">",
            "<em id=\"loop\"><% [1, 2].each do |i| %><%= render \"articles/box\" do %>L<%= i %><% end %><% end %></em>\
<em id=\"nest\"><%= render \"articles/box\" do %><%= render \"articles/box\" do %>N<% end %><% end %></em>\
<% total = 0 %><em id=\"write\"><%= render \"articles/box\" do %>W<% total += 1 %><% end %><%= total %></em>\n    <p class=\"text-gray-700\">",
        )
}

const NESTED_WANT: &[&str] = &[
    "<em id=\"loop\"><u>L1</u><u>L2</u></em>",
    "<em id=\"nest\"><u><u>N</u></u></em>",
    "<em id=\"write\"><u>W</u>1</em>",
];

fn nested_assertions(page: &str) -> String {
    let checks: String = NESTED_WANT.iter().map(|w| format!("raise body[0, 3000] unless body.include?({w:?})\n")).collect();
    format!("{page}\nhello = Article.create!(title: \"Hello\", body: \"Body text here\")\nbody = page(hello.id)\n{checks}puts \"nested yield passed\"\n")
}

#[test]
fn nested_and_looped_block_renders_run() {
    let run = nested_app().run_ruby(&nested_assertions(RUBY_PAGE));
    run.assert_passes();
    assert!(run.stdout.contains("nested yield passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn nested_and_looped_block_renders_run_on_spinel() {
    let run = nested_app().run_spinel_with_db(&nested_assertions(SPINEL_PAGE));
    run.assert_passes();
    assert!(run.stdout.contains("nested yield passed"), "{}", run.stdout);
}

/// What moves with the body keeps its meaning: a `$global` stays the
/// global, a bare `render "badge"` stays the caller's directory's, and a
/// yield argument no parameter takes is still evaluated (here, raises).
fn moved_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/views/shared/_wrap.html.erb", "<s><%= yield %></s>")
        .write("app/views/shared/_loud.html.erb", "<s><%= yield(raise \"evaluated\") %></s>")
        .write("app/views/shared/_badge.html.erb", "S")
        .write("app/views/articles/_badge.html.erb", "A")
        .edit(
            "app/views/articles/show.html.erb",
            "    <p class=\"text-gray-700\">",
            "<em id=\"moved\"><%= render \"shared/wrap\" do %><% $yield_hits = ($yield_hits || 0) + 1 %>G<%= $yield_hits %><%= render \"badge\" %><% end %></em>\
<% if params[:loud] %><%= render \"shared/loud\" do %>never<% end %><% end %>\n    <p class=\"text-gray-700\">",
        )
}

#[test]
fn a_moved_body_keeps_its_meaning() {
    let script = format!(
        "{RUBY_PAGE}\nhello = Article.create!(title: \"Hello\", body: \"Body text here\")\nbody = page(hello.id)\nraise body[0, 3000] unless body.include?(\"<em id=\\\"moved\\\"><s>G1A</s></em>\")\n\
         got = begin\n  Main.dispatch_core({{ \"REQUEST_METHOD\" => \"GET\", \"PATH_INFO\" => \"/articles/#{{hello.id}}\", \"QUERY_STRING\" => \"loud=1\", \"HTTP_ACCEPT\" => \"text/html\" }}, StringIO.new(\"\"))[0]\nrescue RuntimeError => e\n  e.message\nend\nraise got.inspect unless got == \"evaluated\" || got == 500\nputs \"moved passed\"\n"
    );
    let run = moved_app().run_ruby(&script);
    run.assert_passes();
    assert!(run.stdout.contains("moved passed"), "{}", run.stdout);
}

/// campfire's `rooms/closeds/_form`: a partial passing its own local, which
/// its body reads as a call (`tcp`), into a block render; and a block's own
/// local, fresh at each of two yields.
fn caller_local_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/views/shared/_twice.html.erb", "<s><%= yield %>|<%= yield %></s>")
        .write("app/views/articles/_inner.html.erb", "<%= render layout: \"shared/twice\" do %><% memo ||= 0 %><% memo += 1 %><%= tcp %><%= memo %><% end %>")
        .edit(
            "app/views/articles/show.html.erb",
            "    <p class=\"text-gray-700\">",
            "<em id=\"local\"><%= render \"articles/inner\", tcp: \"T\" %></em>\n    <p class=\"text-gray-700\">",
        )
}

#[test]
fn a_caller_partials_local_and_a_fresh_block_local() {
    let script = format!(
        "{RUBY_PAGE}\nhello = Article.create!(title: \"Hello\", body: \"Body text here\")\nbody = page(hello.id)\nraise body[0, 3000] unless body.include?(\"<em id=\\\"local\\\"><s>T1|T1</s></em>\")\nputs \"local passed\"\n"
    );
    let run = caller_local_app().run_ruby(&script);
    run.assert_passes();
    assert!(run.stdout.contains("local passed"), "{}", run.stdout);
}

/// A copy's own block render reads a local its original is passed (`tcp`).
#[test]
fn a_copys_block_render_reads_the_originals_local() {
    let overlay = emit_and_run::real_blog()
        .write("app/views/shared/_wrap.html.erb", "<s><%= yield %></s>")
        .write("app/views/articles/_outer.html.erb", "<%= render layout: \"shared/wrap\" do %><%= tcp %><% end %><%= yield %>")
        .edit(
            "app/views/articles/show.html.erb",
            "    <p class=\"text-gray-700\">",
            "<em id=\"outer\"><%= render \"articles/outer\", tcp: \"T\" do %>B<% end %></em>\n    <p class=\"text-gray-700\">",
        );
    let script = format!(
        "{RUBY_PAGE}\nhello = Article.create!(title: \"Hello\", body: \"Body text here\")\nbody = page(hello.id)\nraise body[0, 3000] unless body.include?(\"<em id=\\\"outer\\\"><s>T</s>B</em>\")\nputs \"outer passed\"\n"
    );
    let run = overlay.run_ruby(&script);
    run.assert_passes();
    assert!(run.stdout.contains("outer passed"), "{}", run.stdout);
}

/// Left to the captured string: a call of a name only some renders of the
/// caller partial pass (it may be a helper, which Rails calls only at the
/// yield), and one the block also binds in a nested scope.
#[test]
fn unsure_caller_locals_are_not_specialized() {
    for (inner, renders) in [
        ("<%= render layout: \"shared/wrap\" do %><%= tcp %><% end %>", "<%= render \"articles/inner\", tcp: \"T\" %><%= render \"articles/inner\" %>"),
        (
            "<%= render layout: \"shared/wrap\" do %><% [1].each do |tcp| %><%= tcp %><% end %><%= tcp %><% end %>",
            "<%= render \"articles/inner\", tcp: \"T\" %>",
        ),
        (
            "<%= render layout: \"shared/wrap\" do %><% total += 1 %><%= total %><% end %>",
            "<%= render \"articles/inner\", total: 5 %><%= render \"articles/inner\" %>",
        ),
    ] {
        let (emitted, _) = emit_and_run::real_blog()
            .write("app/views/shared/_wrap.html.erb", "<s><%= yield %></s>")
            .write("app/views/articles/_inner.html.erb", inner)
            .edit("app/views/articles/show.html.erb", "    <p class=\"text-gray-700\">", &format!("{renders}\n    <p class=\"text-gray-700\">"))
            .emit(roundhouse::project::BuildTarget::Ruby);
        let shared = emitted.join("app/views/shared");
        let copies: Vec<String> = std::fs::read_dir(&shared)
            .unwrap_or_else(|e| panic!("read {}: {e}", shared.display()))
            .map(|e| e.expect("directory entry").file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("__yield"))
            .collect();
        assert!(copies.is_empty(), "{inner}: {copies:?}");
    }
}

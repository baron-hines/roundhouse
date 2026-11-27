//! A partial rendered from a template resolves in that template's format
//! first. Rails prepends the rendering template's format to the lookup
//! (`TemplateRenderer#render` → `prepend_formats(template.format)`), so
//! `render "note"` in `banner.turbo_stream.erb` finds
//! `_note.turbo_stream.erb` ahead of `_note.html.erb`. The html partial
//! is the fallback a Turbo request leaves in the list: its `Accept`
//! carries `text/html`, and the `turbo_stream` builder adds `:html` too.
//!
//! The variants lower beside each other (`note_turbo_stream` and
//! `note`), and every render call site named the html one, so a
//! turbo_stream-only partial was a call to a method nothing defines.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

const CONTROLLER_TEST: &str = "test/controllers/articles_controller_test.rb";

const HTML_NOTE: &str = "<p id=\"note\">Html note</p>\n";

const STREAM_NOTE: &str = "<p id=\"note\">Stream note</p>\n";

/// The blog with a `banner` action whose only template is
/// `banner.turbo_stream.erb` holding `template`, and a test that asks for
/// it as a turbo stream and expects `expect` in the body and not `absent`.
fn banner_app(template: &str, expect: &str, absent: &str) -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/views/articles/banner.turbo_stream.erb", template)
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  \
             get \"articles/:id/banner\", to: \"articles#banner\", as: :banner_article\n",
        )
        .edit(
            "app/controllers/articles_controller.rb",
            "  # GET /articles/new\n",
            "  def banner\n  end\n\n  # GET /articles/new\n",
        )
        .edit(
            CONTROLLER_TEST,
            "  test \"should get edit\" do\n",
            &format!(
                "  test \"the turbo stream renders the note\" do\n    \
                 get banner_article_url(@article, format: :turbo_stream)\n    \
                 assert_includes response.body, {expect:?}\n    \
                 assert_not_includes response.body, {absent:?}\n  end\n\n  \
                 test \"should get edit\" do\n"
            ),
        )
}

#[test]
fn a_turbo_stream_template_renders_a_turbo_stream_only_partial() {
    banner_app("<%= render \"note\" %>\n", "Stream note", "Html note")
        .write("app/views/articles/_note.turbo_stream.erb", STREAM_NOTE)
        .run_test(CONTROLLER_TEST)
        .assert_passes();
}

#[test]
fn a_turbo_stream_template_falls_back_to_the_html_partial() {
    banner_app("<%= render \"note\" %>\n", "Html note", "Stream note")
        .write("app/views/articles/_note.html.erb", HTML_NOTE)
        .run_test(CONTROLLER_TEST)
        .assert_passes();
}

#[test]
fn a_turbo_stream_template_prefers_the_turbo_stream_partial() {
    banner_app("<%= render \"note\" %>\n", "Stream note", "Html note")
        .write("app/views/articles/_note.html.erb", HTML_NOTE)
        .write("app/views/articles/_note.turbo_stream.erb", STREAM_NOTE)
        .run_test(CONTROLLER_TEST)
        .assert_passes();
}

/// A partial lookup finds partials only: `banner.turbo_stream.erb` is the
/// template rendering `render "banner"`, not a turbo_stream `_banner`.
#[test]
fn a_partial_sharing_its_templates_name_renders_the_html_partial() {
    banner_app("<%= render \"banner\" %>\n", "Html note", "Stream note")
        .write("app/views/articles/_banner.html.erb", HTML_NOTE)
        .run_test(CONTROLLER_TEST)
        .assert_passes();
}

/// Nor does `show.html.erb` stand in for an html `_show` the list names.
#[test]
fn a_formats_list_skips_an_action_template_of_the_partials_name() {
    banner_app(
        "<%= render partial: \"show\", formats: [:html, :turbo_stream] %>\n",
        "Stream note",
        "Html note",
    )
    .write("app/views/articles/_show.turbo_stream.erb", STREAM_NOTE)
    .run_test(CONTROLLER_TEST)
    .assert_passes();
}

/// A template lookup finds templates only, past a turbo_stream partial
/// of the same name.
#[test]
fn a_template_render_skips_a_partial_of_the_templates_name() {
    banner_app(
        "<%= render template: \"articles/teaser\" %>\n",
        "Html note",
        "Stream note",
    )
    .write("app/views/articles/teaser.html.erb", HTML_NOTE)
    .write("app/views/articles/_teaser.turbo_stream.erb", STREAM_NOTE)
    .run_test(CONTROLLER_TEST)
    .assert_passes();
}

/// `render partial:, collection:` takes the same lookup, once per element.
#[test]
fn a_turbo_stream_collection_render_prefers_the_turbo_stream_partial() {
    banner_app(
        "<%= render partial: \"note\", collection: Article.all %>\n",
        "Stream note",
        "Html note",
    )
    .write("app/views/articles/_note.html.erb", HTML_NOTE)
    .write("app/views/articles/_note.turbo_stream.erb", STREAM_NOTE)
    .run_test(CONTROLLER_TEST)
    .assert_passes();
}

/// An html template looks up html first, beside a turbo_stream twin.
#[test]
fn an_html_template_renders_the_html_partial() {
    emit_and_run::real_blog()
        .write("app/views/articles/_note.html.erb", HTML_NOTE)
        .write("app/views/articles/_note.turbo_stream.erb", STREAM_NOTE)
        .edit(
            "app/views/articles/show.html.erb",
            "  <h1 class=\"font-bold text-4xl\"><%= @article.title %></h1>\n",
            "  <h1 class=\"font-bold text-4xl\"><%= @article.title %></h1>\n  <%= render \"note\" %>\n",
        )
        .edit(
            CONTROLLER_TEST,
            "    assert_select \"h2\", \"Comments\"\n",
            "    assert_select \"h2\", \"Comments\"\n    \
             assert_includes response.body, \"Html note\"\n    \
             assert_not_includes response.body, \"Stream note\"\n",
        )
        .run_test(CONTROLLER_TEST)
        .assert_passes();
}

/// turbo-rails renders a `turbo_stream.<action> target, partial:` with
/// `formats: [:html]` (`TagBuilder#render_template`), whatever template
/// it is written in.
#[test]
fn the_turbo_stream_builder_renders_the_html_partial() {
    banner_app(
        "<%= turbo_stream.update \"note\", partial: \"articles/note\" %>\n",
        "Html note",
        "Stream note",
    )
    .write("app/views/articles/_note.html.erb", HTML_NOTE)
    .write("app/views/articles/_note.turbo_stream.erb", STREAM_NOTE)
    .run_test(CONTROLLER_TEST)
    .assert_passes();
}

/// `formats:` replaces the lookup's formats for that render, and is not
/// a local the partial is handed.
#[test]
fn an_explicit_formats_option_picks_that_partial() {
    banner_app(
        "<%= render partial: \"note\", formats: [:html] %>\n",
        "Html note",
        "Stream note",
    )
    .write("app/views/articles/_note.html.erb", HTML_NOTE)
    .write("app/views/articles/_note.turbo_stream.erb", STREAM_NOTE)
    .run_test(CONTROLLER_TEST)
    .assert_passes();
}

/// An explicit list is walked in order, the first format with a variant
/// winning; `:xml` has none here, so `:turbo_stream` is the one found.
#[test]
fn an_explicit_formats_list_falls_through_to_its_next_format() {
    banner_app(
        "<%= render partial: \"note\", formats: [:xml, :turbo_stream] %>\n",
        "Stream note",
        "Html note",
    )
    .write("app/views/articles/_note.html.erb", HTML_NOTE)
    .write("app/views/articles/_note.turbo_stream.erb", STREAM_NOTE)
    .run_test(CONTROLLER_TEST)
    .assert_passes();
}

/// `render layout:` with a block takes `formats:` the same way: Rails
/// resolves the layout partial inside the replaced lookup.
#[test]
fn an_explicit_formats_option_picks_that_layout() {
    banner_app(
        "<%= render layout: \"wrapper\", formats: [:html] do %>Body<% end %>\n",
        "Html wrapper",
        "Stream wrapper",
    )
    .write("app/views/articles/_wrapper.html.erb", "<div>Html wrapper <%= yield %></div>\n")
    .write(
        "app/views/articles/_wrapper.turbo_stream.erb",
        "<div>Stream wrapper <%= yield %></div>\n",
    )
    .run_test(CONTROLLER_TEST)
    .assert_passes();
}

const CACHE_TEST: &str = "test/controllers/note_caches_controller_test.rb";

/// The show page and the banner stream both render `render`, over notes
/// that each `cache note` in their own format, with fragment caching on
/// and one store behind the controller and the collection cache. The
/// test warms the html fragments, then the stream's, then reads the
/// page again: each format finds only its own markup, one fragment per
/// element per format.
fn cached_notes_app(render: &str) -> emit_and_run::Overlay {
    banner_app(render, "Stream note", "Html note")
        .write(
            "app/views/articles/_note.html.erb",
            "<% cache note do %><p id=\"note\">Html note</p><% end %>\n",
        )
        .write(
            "app/views/articles/_note.turbo_stream.erb",
            "<% cache note do %><p id=\"note\">Stream note</p><% end %>\n",
        )
        .edit(
            "app/views/articles/show.html.erb",
            "  <h1 class=\"font-bold text-4xl\"><%= @article.title %></h1>\n",
            &format!("  <h1 class=\"font-bold text-4xl\"><%= @article.title %></h1>\n  {render}"),
        )
        .write(
            CACHE_TEST,
            r#"require "test_helper"

class NoteCachesControllerTest < ActionDispatch::IntegrationTest
  setup do
    @article = articles(:one)
    # Past the length a cached collection renders uncached (MAX_UNCACHED_COLLECTION_LENGTH).
    8.times { |i| Article.create!(title: "More #{i}", body: "A sufficiently long article body.") }
    @store = ActiveSupport::Cache::MemoryStore.new
    @previous_cache_store = ActionController::Base.cache_store
    @previous_collection_cache = ActionView::PartialRenderer.collection_cache
    ActionController::Base.cache_store = @store
    ActionView::PartialRenderer.collection_cache = @store
    ActionController::Base.perform_caching = true
  end

  teardown do
    ActionController::Base.perform_caching = false
    ActionController::Base.cache_store = @previous_cache_store
    ActionView::PartialRenderer.collection_cache = @previous_collection_cache
  end

  test "each format is served its own fragments" do
    get article_url(@article)
    assert_includes response.body, "Html note"
    get banner_article_url(@article, format: :turbo_stream)
    assert_includes response.body, "Stream note"
    assert_not_includes response.body, "Html note"
    get article_url(@article)
    assert_includes response.body, "Html note"
    assert_not_includes response.body, "Stream note"
    assert_includes @store.inspect, "entries=#{2 * Article.count},"
  end
end
"#,
        )
}

/// A partial's `cache note` keys its fragment under its own variant, as
/// Rails' digest path of that variant does.
#[test]
fn a_partials_cache_block_keeps_each_formats_fragment() {
    cached_notes_app("<%= render partial: \"note\", collection: Article.all %>\n")
        .run_test(CACHE_TEST)
        .assert_passes();
}

/// A `cached: true` collection keys each element under the variant the
/// render resolves, the same key the partial's own block files under.
#[test]
fn a_cached_collection_keeps_each_formats_fragments() {
    cached_notes_app("<%= render partial: \"note\", collection: Article.all, cached: true %>\n")
        .run_test(CACHE_TEST)
        .assert_passes();
}

/// A callable `cached:` key goes under the same variant's name.
#[test]
fn a_keyed_cached_collection_keeps_each_formats_fragments() {
    cached_notes_app(
        "<%= render partial: \"note\", collection: Article.all, cached: ->(note) { note } %>\n",
    )
    .run_test(CACHE_TEST)
    .assert_passes();
}

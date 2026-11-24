//! A partial rendered from another partial takes its locals' types from
//! that render: the blog's `articles/_form` handing `form.object` and a
//! field to an `articles/_field_error` partial types `resource` as the
//! Article and `field` as a Symbol, as a view's render types its partial's.
//! Only views' and layouts' renders seeded partials once, so the inner
//! partial's locals were unknown and its `errors[field]` read went
//! unresolved.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::ident::Symbol;
use roundhouse::project::BuildTarget;
use roundhouse::ty::Ty;

const CONTROLLER_TEST: &str = "test/controllers/articles_controller_test.rb";

const FORM: &str = "app/views/articles/_form.html.erb";

const TITLE_FIELD: &str = "    <%= form.label :title %>\n";

const FIELD_ERROR: &str = "app/views/articles/_field_error.html.erb";

const FIELD_ERROR_ERB: &str = "<% if resource.errors[field].any? %>\n  \
     <p class=\"field-error\"><%= resource.errors.full_messages_for(field).to_sentence %></p>\n\
     <% end %>\n";

const INVALID_ASSERT: &str = "      post articles_url, params: { article: { title: \"\", body: \"\" } }\n    end\n\n    assert_response :unprocessable_entity\n";

/// The blog's form rendering the field-error partial under its title
/// field, and the invalid create asserting what that partial rendered.
fn field_errors() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(FIELD_ERROR, FIELD_ERROR_ERB)
        .edit(
            FORM,
            TITLE_FIELD,
            &format!("{TITLE_FIELD}    <%= render \"field_error\", resource: form.object, field: :title %>\n"),
        )
        .edit(
            CONTROLLER_TEST,
            INVALID_ASSERT,
            &format!(
                "{INVALID_ASSERT}    assert_includes response.body, \"<p class=\\\"field-error\\\">Title can&#39;t be blank</p>\"\n"
            ),
        )
}

#[test]
fn a_partial_s_render_types_the_locals_of_the_partial_it_renders() {
    let (_emitted, app, errors) = field_errors().emit_with_app(BuildTarget::Ruby);
    assert!(errors.is_empty(), "emit errors: {errors:?}");
    let locals = app
        .partial_local_types
        .get(&Symbol::from("articles/_field_error"))
        .expect("articles/_field_error is seeded from articles/_form's render");
    match locals.get(&Symbol::from("resource")) {
        Some(Ty::Class { id, .. }) => assert_eq!(id.0.as_str(), "Article"),
        other => panic!("expected resource : Article, got {other:?}"),
    }
    assert_eq!(locals.get(&Symbol::from("field")), Some(&Ty::Sym));
}

#[test]
fn a_partial_rendered_from_a_partial_reads_its_record_s_errors() {
    field_errors().run_test(CONTROLLER_TEST).assert_passes();
}

/// The type `partial` was handed for `local`, from an overlay that
/// renders `outer` under the blog form's title field.
fn local_type(outer: &str, partials: &[(&str, &str)], partial: &str, local: &str) -> Option<Ty> {
    let mut overlay = emit_and_run::real_blog().edit(FORM, TITLE_FIELD, &format!("{TITLE_FIELD}    {outer}\n"));
    for (path, erb) in partials {
        overlay = overlay.write(path, erb);
    }
    let (_emitted, app, errors) = overlay.emit_with_app(BuildTarget::Ruby);
    assert!(errors.is_empty(), "emit errors: {errors:?}");
    app.partial_local_types.get(&Symbol::from(partial)).and_then(|locals| locals.get(&Symbol::from(local))).cloned()
}

/// A partial that renders itself hands on a local it has not typed yet;
/// that site, harvested after `_form`'s, does not erase `_form`'s Article.
#[test]
fn a_later_render_that_cannot_type_a_local_keeps_the_earlier_type() {
    let ty = local_type(
        "<%= render \"node\", resource: form.object, depth: 0 %>",
        &[(
            "app/views/articles/_node.html.erb",
            "<p><%= resource.title %></p>\n<% if depth < 1 %><%= render \"node\", resource: resource, depth: depth + 1 %><% end %>\n",
        )],
        "articles/_node",
        "resource",
    );
    assert!(matches!(&ty, Some(Ty::Class { id, .. }) if id.0.as_str() == "Article"), "expected resource : Article, got {ty:?}");
}

/// Two partials down, a container of the middle partial's local types
/// only once the middle partial is typed: `Array[unknown]` does not
/// hold the leaf's local against the `Array[Article]` a later round reads.
#[test]
fn a_container_local_two_partials_down_takes_its_element_type() {
    let ty = local_type(
        "<%= render \"middle\", resource: form.object %>",
        &[
            ("app/views/articles/_middle.html.erb", "<%= render \"leaf\", items: [resource] %>\n"),
            ("app/views/articles/_leaf.html.erb", "<% items.each do |item| %><p><%= item.title %></p><% end %>\n"),
        ],
        "articles/_leaf",
        "items",
    );
    assert!(
        matches!(&ty, Some(Ty::Array { elem }) if matches!(&**elem, Ty::Class { id, .. } if id.0.as_str() == "Article")),
        "expected items : Array[Article], got {ty:?}"
    );
}

/// A partial's formats share its name: retyping the `.html.erb` card a
/// nested render seeds keeps its own body, not its `.json.jbuilder`
/// sibling's. campfire's `messages/boosts/_boost` is both.
#[test]
fn a_retyped_partial_keeps_its_own_format_s_body() {
    emit_and_run::real_blog()
        .write("app/views/articles/_card.html.erb", "<p class=\"card\" data-new=\"<%= article.new_record? %>\">Card</p>\n")
        .write("app/views/articles/_card.json.jbuilder", "json.title article.title\n")
        .edit(FORM, TITLE_FIELD, &format!("{TITLE_FIELD}    <%= render \"card\", article: form.object %>\n"))
        .edit(
            CONTROLLER_TEST,
            INVALID_ASSERT,
            &format!("{INVALID_ASSERT}    assert_includes response.body, \"<p class=\\\"card\\\" data-new=\\\"true\\\">Card</p>\"\n"),
        )
        .run_test(CONTROLLER_TEST)
        .assert_passes();
}

fn is_array_of_article(ty: &Option<Ty>) -> bool {
    matches!(ty, Some(Ty::Array { elem }) if matches!(&**elem, Ty::Class { id, .. } if id.0.as_str() == "Article"))
}

/// Two sites in one partial hand the leaf the same local: the later
/// site's `Array[unknown]` (`items: []`) does not erase the earlier
/// site's `Array[Article]`, round after round.
#[test]
fn a_later_partly_unknown_container_keeps_the_earlier_element_type() {
    let ty = local_type(
        "<%= render \"middle\", resource: form.object %>",
        &[
            (
                "app/views/articles/_middle.html.erb",
                "<%= render \"leaf\", items: [resource] %>\n<%= render \"leaf\", items: [] %>\n",
            ),
            ("app/views/articles/_leaf.html.erb", "<% items.each do |item| %><p><%= item.title %></p><% end %>\n"),
        ],
        "articles/_leaf",
        "items",
    );
    assert!(is_array_of_article(&ty), "expected items : Array[Article], got {ty:?}");
}

/// A view's `items: []` seeds the leaf's local as `Array[unknown]`;
/// that partly known seed gives way to the `Array[Article]` a nested
/// render reads, where a fully known seed would hold.
#[test]
fn a_partly_unknown_view_seed_gives_way_to_a_nested_known_type() {
    let render_form = "  <%= render \"form\", article: @article %>\n";
    let (_emitted, app, errors) = emit_and_run::real_blog()
        .edit("app/views/articles/new.html.erb", render_form, &format!("{render_form}  <%= render \"leaf\", items: [] %>\n"))
        .edit(FORM, TITLE_FIELD, &format!("{TITLE_FIELD}    <%= render \"middle\", resource: form.object %>\n"))
        .write("app/views/articles/_middle.html.erb", "<%= render \"leaf\", items: [resource] %>\n")
        .write("app/views/articles/_leaf.html.erb", "<% items.each do |item| %><p><%= item.title %></p><% end %>\n")
        .emit_with_app(BuildTarget::Ruby);
    assert!(errors.is_empty(), "emit errors: {errors:?}");
    let ty = app
        .partial_local_types
        .get(&Symbol::from("articles/_leaf"))
        .and_then(|locals| locals.get(&Symbol::from("items")))
        .cloned();
    assert!(is_array_of_article(&ty), "expected items : Array[Article], got {ty:?}");
}

/// A form whose record's class names its directory (`features/_form`
/// handed a `Feature` `article`) where neither is a route still tries
/// the local's name: campfire's `opens/_form` shape, a `Rooms::Open`
/// `room` posting to `rooms_path`. The class matching the directory
/// stopped the search and kept the unrouted `features_path`.
#[test]
fn a_form_whose_class_names_an_unrouted_directory_posts_to_its_local_s_route() {
    let heading = "  <h1 class=\"font-bold text-4xl\"><%= @article.title %></h1>\n";
    let (emitted, _app, errors) = emit_and_run::real_blog()
        .write("app/models/feature.rb", "class Feature < Article\nend\n")
        .write("app/views/features/_form.html.erb", "<%= form_with model: article do |form| %>\n  <%= form.text_field :title %>\n<% end %>\n")
        .edit("app/views/articles/show.html.erb", heading, &format!("{heading}  <%= render \"features/form\", article: Feature.new %>\n"))
        .emit_with_app(BuildTarget::Ruby);
    assert!(errors.is_empty(), "emit errors: {errors:?}");
    let views = walk(&emitted);
    let form = views
        .iter()
        .find(|(path, _)| path.contains("features") && path.contains("form"))
        .unwrap_or_else(|| panic!("no emitted features form among {:?}", views.iter().map(|(p, _)| p).collect::<Vec<_>>()));
    assert!(!form.1.contains("features_path") && !form.1.contains("feature_path"), "unrouted helper in {}:\n{}", form.0, form.1);
    assert!(form.1.contains("articles_path"), "expected articles_path in {}:\n{}", form.0, form.1);
}

fn walk(dir: &std::path::Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else if path.extension().is_some_and(|e| e == "rb") {
            out.push((path.display().to_string(), std::fs::read_to_string(&path).unwrap_or_default()));
        }
    }
    out
}

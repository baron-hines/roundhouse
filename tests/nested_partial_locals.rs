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

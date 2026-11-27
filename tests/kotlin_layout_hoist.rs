//! The Kotlin layout builds its stylesheet links and importmap tags once
//! per process, not on every request. `hoist_pure_helper_constants`
//! (shared lowering, also run by the Ruby/Spinel emit) lifts a pure helper
//! call over literals into a view-module constant; the Kotlin view object
//! renders it as a `by lazy` property. Rebuilding them per request cost
//! the real-blog HTML routes 8-11% of their throughput. The byte-identical
//! page is `kotlin_toolchain` + `scripts/smoke kotlin`.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

use roundhouse::project::BuildTarget;

#[test]
fn the_kotlin_layout_reads_its_static_head_tags_from_lazy_constants() {
    let (emitted, errors) = emit_and_run::real_blog().emit(BuildTarget::Kotlin);
    assert!(errors.is_empty(), "{errors:?}");
    let layouts = std::fs::read_to_string(emitted.join("src/main/kotlin/app/views/Layouts.kt")).unwrap();

    assert!(
        layouts.contains("val HOISTED_JAVASCRIPT_IMPORTMAP_TAGS by lazy { ViewHelpers.javascriptImportmapTags(Importmap.pins(), Importmap.entry()) }"),
        "importmap tags are not a lazy constant:\n{layouts}"
    );
    assert!(layouts.contains("io.append(HOISTED_JAVASCRIPT_IMPORTMAP_TAGS)"), "{layouts}");
    // The two links and the "\n" between them fold into one constant, so
    // the concatenation is not redone per request either.
    assert!(
        layouts.contains("val HOISTED_CONCAT by lazy { HOISTED_STYLESHEET_LINK_TAG_APPLICATION + \"\\n\" + HOISTED_STYLESHEET_LINK_TAG_TAILWIND }"),
        "{layouts}"
    );
    assert!(layouts.contains("io.append(HOISTED_CONCAT)"), "{layouts}");

    // The per-request half stays per request: the CSRF token is minted for
    // each page, and `fun application` no longer calls the hoisted helpers.
    let body = &layouts[layouts.find("fun application(").expect("application layout")..];
    let body = &body[..body.find("\n    }\n").expect("end of application")];
    assert!(body.contains("ViewHelpers.csrfMetaTags()"), "{body}");
    for helper in ["javascriptImportmapTags", "stylesheetLinkTag"] {
        assert!(!body.contains(helper), "`{helper}` still runs per request:\n{body}");
    }
}

/// A template at the top of `app/views/` lowers to the bare `Views`
/// module, not a `Views::<Dir>` one; its hoisted constant must still be
/// declared on the object, or the reference to it does not compile
/// (CodeRabbit on #2022).
#[test]
fn a_top_level_templates_hoisted_constant_is_declared() {
    let (emitted, errors) = emit_and_run::real_blog()
        .write("app/views/about.html.erb", "<%= stylesheet_link_tag \"about\" %>\n")
        .emit(BuildTarget::Kotlin);
    assert!(errors.is_empty(), "{errors:?}");
    let views = std::fs::read_to_string(emitted.join("src/main/kotlin/app/views/Views.kt"))
        .expect("the top-level template's `Views` object");
    assert!(views.contains("object Views {"), "{views}");
    let reference = views.find("io.append(HOISTED_STYLESHEET_LINK_TAG_ABOUT)").expect("the hoisted stylesheet link");
    assert!(
        views[..reference].contains("val HOISTED_STYLESHEET_LINK_TAG_ABOUT by lazy { ViewHelpers.stylesheetLinkTag(\"about\""),
        "referenced but not declared:\n{views}"
    );
}

/// Only a view module's constants are rendered on its object: a runtime
/// module's constants are already file-scope `val`s, and a second copy
/// on the object would shadow them.
#[test]
fn runtime_module_constants_are_not_duplicated_onto_the_object() {
    let (emitted, errors) = emit_and_run::real_blog().emit(BuildTarget::Kotlin);
    assert!(errors.is_empty(), "{errors:?}");
    let base = std::fs::read_to_string(emitted.join("src/main/kotlin/ActionControllerBase.kt")).unwrap();
    assert_eq!(base.matches("val FORGERY_SLOT").count(), 1, "{base}");
    assert!(!base.contains(" by lazy {"), "{base}");
}

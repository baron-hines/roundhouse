//! HAML shortcut-class + hash `class:` merge (Haml 7.5.1 semantics):
//! `.g{ class: k }` folds the `.g` shortcut and the hash `class:` value
//! together through the runtime `haml_class` helper instead of dropping
//! the shortcut. See `src/haml.rs`'s `element` and
//! `runtime/ruby/action_view/view_helpers.rb`'s `haml_class`.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

#[path = "support/haml_class_shortcut_merge.rs"]
mod contract;
use contract::{app, render};

/// Falsy branch: the dynamic value drops out everywhere, leaving only
/// the static shortcut class (never a bare `class=""`).
#[test]
fn falsy_dynamic_values_leave_only_the_static_classes() {
    let run = app().run_ruby(&render("0", "0"));
    run.assert_passes();
    assert_eq!(
        run.stdout.trim_end(),
        concat!(
            "<div class=\"gadget alert\"></div>",
            "<div class=\"gadget\"></div>",
            "<div class=\"gadget off\"></div>",
            "<div class=\"gadget\"></div>",
            "<a href=\"/w\" class=\"gadget alert\" title=\"t\"></a>",
            "<div class=\"gadget\" title=\"t\"></div>",
            "<div class=\"gadget\"></div>",
        ),
        "got:\n{}",
        run.stdout
    );
}

/// Truthy branch: each dynamic value appends after the static class.
#[test]
fn truthy_dynamic_values_append_after_the_static_class() {
    let run = app().run_ruby(&render("1", "1"));
    run.assert_passes();
    assert_eq!(
        run.stdout.trim_end(),
        concat!(
            "<div class=\"gadget alert\"></div>",
            "<div class=\"gadget gadget--rtl\"></div>",
            "<div class=\"gadget on\"></div>",
            "<div class=\"gadget gadget--hot gadget--cold\"></div>",
            "<a href=\"/w\" class=\"gadget alert\" title=\"t\"></a>",
            "<div class=\"gadget\" title=\"t\"></div>",
            "<div class=\"gadget\"></div>",
        ),
        "got:\n{}",
        run.stdout
    );
}

/// Byte-identical guard: an element with a shortcut class and NO hash
/// `class:` key emits exactly what it does without the merge feature —
/// `.gadget{ title: 't' }` and plain `.gadget`, the last two elements
/// in every render above, are asserted there already; this pins them
/// on their own so a future change to the merge path that regresses
/// the no-`class:`-key case fails here specifically.
#[test]
fn an_element_without_a_hash_class_key_is_unaffected() {
    let run = app().run_ruby(&render("0", "0"));
    run.assert_passes();
    assert!(
        run.stdout.contains("<div class=\"gadget\" title=\"t\"></div><div class=\"gadget\"></div>"),
        "got:\n{}",
        run.stdout
    );
}

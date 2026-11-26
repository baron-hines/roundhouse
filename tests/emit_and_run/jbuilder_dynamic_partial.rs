//! `json.partial!` whose path is built at run time, emitted for CRuby
//! and run through the controller; the Rails side and the shared app are
//! in `tests/support/jbuilder_dynamic_partial.rs`.

use super::jbuilder_dynamic_partial as app;

/// `json.partial! "widgets/#{@widget.kind}", widget: @widget` renders
/// the partial the kind names, as the whole template and inside a key,
/// and a kind with no partial raises `ActionView::MissingTemplate`. It
/// was refused as a call with unrecognised arguments (before that, it
/// rendered `{}`).
#[test]
fn a_partial_path_built_at_run_time_renders_the_partial_it_names() {
    let script = format!(
        r#"require_relative "app/controllers/widgets_controller"
{rows}def render(action, widget)
  controller = WidgetsController.new
  controller.params = {{"id" => widget.id.to_s}}
  controller.process_action(action)
  controller.body
end
{checks}"#,
        rows = app::create_rows(),
        checks = app::checks(),
    );
    app::app().run_ruby(&script).assert_passes();
}

/// The fallback raises `ActionView::MissingTemplate`, which only the
/// ruby family and Spinel ship. Any other target refuses the call at
/// the `partial!`, as it refused the call before, instead of emitting a
/// reference to a class its runtime does not define (`throw new
/// MissingTemplate(…)` on TypeScript, `::ActionView::MissingTemplate`
/// on Crystal).
#[test]
fn a_partial_path_built_at_run_time_is_refused_where_missing_template_is_not_shipped() {
    use roundhouse::project::BuildTarget;
    for (target, name) in [(BuildTarget::Typescript, "typescript"), (BuildTarget::Crystal, "crystal")] {
        let (_, errors) = app::app().emit(target);
        let want = format!("ActionView::MissingTemplate is not available as a class/module value on {name}");
        let refusals: Vec<&String> = errors.iter().filter(|e| e.contains(&want)).collect();
        // One per `partial!`: `show` and `wrapped`.
        assert_eq!(refusals.len(), 2, "{name}: {errors:#?}");
    }
    for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
        let (_, errors) = app::app().emit(target);
        assert!(!errors.iter().any(|e| e.contains("MissingTemplate")), "{target:?}: {errors:#?}");
    }
}

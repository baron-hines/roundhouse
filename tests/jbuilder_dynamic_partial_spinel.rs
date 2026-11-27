//! `json.partial!` whose path is built at run time, compiled by Spinel.
//! The CRuby side is `emit_and_run/jbuilder_dynamic_partial.rs`; the app
//! and the Rails 8.1.4 + jbuilder 2.15.1 output are in
//! `support/jbuilder_dynamic_partial.rs`.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;
#[path = "support/jbuilder_dynamic_partial.rs"]
mod jbuilder_dynamic_partial;

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_partial_path_built_at_run_time_under_spinel() {
    let script = format!(
        r#"{rows}def render(action, widget)
  if action == :show
    Views::Widgets.show_json(widget)
  else
    Views::Widgets.wrapped_json(widget)
  end
end
{checks}"#,
        rows = jbuilder_dynamic_partial::create_rows(),
        checks = jbuilder_dynamic_partial::checks(),
    );
    jbuilder_dynamic_partial::app().run_spinel_with_db(&script).assert_passes();
}

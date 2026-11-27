use super::emit_and_run;

/// An ApplicationController hook reading an ivar only a subclass's action
/// writes (lobsters' exception-notifier hook reads `@requested_path`, which
/// AboutController#four_oh_four sets). Rails reads the subclass's value,
/// or nil where no action wrote it; this was an ivar_unresolved error.
fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "app/controllers/application_controller.rb",
            "  stale_when_importmap_changes\n",
            "  stale_when_importmap_changes

  prepend_before_action :note_request

  def note_request
    @noted = { requested_path: @requested_path, full: request.original_fullpath }
  end
",
        )
        .edit(
            "app/controllers/articles_controller.rb",
            "  def index\n",
            "  def index\n    @requested_path = request.original_fullpath\n",
        )
}

#[test]
fn an_ancestor_reads_an_ivar_its_descendants_write() {
    let (_emitted, errors) = app().emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    app().run_test("test/controllers/articles_controller_test.rb").assert_passes();
}

/// A strict target's ancestor controller has no field for it.
#[test]
fn a_strict_target_reports_the_ancestor_s_read() {
    let (_emitted, errors) = app().emit(roundhouse::project::BuildTarget::Rust);
    let found: Vec<&String> = errors.iter().filter(|e| e.contains("ivar written only by a subclass not supported (rust)")).collect();
    assert_eq!(found.len(), 1, "{errors:#?}");
}

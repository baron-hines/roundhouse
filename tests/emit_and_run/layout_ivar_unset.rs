//! A layout ivar that only some of the actions it wraps assign reads nil
//! for the others, as in Rails. lobsters' layout writes
//! `<h1 class="<%= @title_class %>">`, which only UsersController sets;
//! typed from the assigning actions alone the slot was a `Symbol`, and
//! Spinel rendered every other page's nil as `class="x"`.

use super::emit_and_run;
use roundhouse::project::BuildTarget;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("app/views/layouts/application.html.erb", "  <body>\n", "  <body>\n    <h1 class=\"<%= @title_class %>\">T</h1>\n")
        .edit("app/controllers/articles_controller.rb", "  def show\n", "  def show\n    @title_class = :new_user\n")
}

const ASSERTIONS: &str = r##"plain = Views::Layouts.application("", nil)
marked = Views::Layouts.application("", :new_user)
raise plain unless plain.include?("<h1 class=\"\">T</h1>")
raise marked unless marked.include?("<h1 class=\"new_user\">T</h1>")
puts "layout ivar unset passed"
"##;

#[test]
fn an_ivar_some_actions_leave_unset_is_nullable_in_the_layout() {
    let (emitted, errors) = app().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    let sig = std::fs::read_to_string(emitted.join("app/views/layouts/application.rbs")).expect("layout rbs");
    assert!(sig.contains("Symbol? title_class"), "{sig}");
}

#[test]
fn layout_ivar_unset_runs() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("layout ivar unset passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn layout_ivar_unset_runs_on_spinel() {
    let run = app().run_spinel_with_rbs(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("layout ivar unset passed"), "{}", run.stdout);
}

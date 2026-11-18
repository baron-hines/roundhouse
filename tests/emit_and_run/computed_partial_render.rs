//! maybe's `render partial: permitted_import_configuration_path(@import)`: a
//! partial whose name a helper computes as the page renders. The lowering
//! dispatches a runtime name only over the partials a controller assigns an
//! ivar, so this one was emitted as a bare `render(…)` the view's module has
//! no method for, and the page raised NoMethodError. It is reported instead.

use super::emit_and_run;

#[test]
fn a_computed_partial_name_is_reported() {
    let (_emitted, errors) = emit_and_run::real_blog()
        .write("app/helpers/kinds_helper.rb", "module KindsHelper\n  def kind_partial(article)\n    \"articles/kinds/#{article.title.downcase}\"\n  end\nend\n")
        .write("app/views/articles/kinds/_hello.html.erb", "<p>hello <%= article.title %></p>\n")
        .edit(
            "app/views/articles/show.html.erb",
            "    <p class=\"text-gray-700\">",
            "<%= render partial: kind_partial(@article), locals: { article: @article } %>\n    <p class=\"text-gray-700\">",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("render of a computed partial")), "{errors:#?}");
}

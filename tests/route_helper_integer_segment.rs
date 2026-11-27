//! A path segment that one call fills with a String is declared
//! `String`, and an Integer another call passes (a foreign key,
//! `comment.article_id`) reaches it as its `to_s`. Rails renders both
//! the same: `parts_path("1")` and `parts_path(1)` are "/widgets/1/parts".
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;

const CONTROLLER: &str = "class PartsController < ApplicationController
  def index
    render json: part_paths(current_widget)
  end

  def part_paths(widget_id)
    [widget_parts_path(widget_id), widget_parts_path(widget_id.to_s)]
  end

  def show
    render json: [top_paths(current_widget), maybe_top_paths(current_widget, params[:flag] == \"1\")]
  end

  def top_paths(length)
    [top_path(length), top_path(length.to_s), top_path]
  end

  def maybe_top_paths(length, flag)
    [top_path(flag ? length : nil), top_path(flag ? length.to_s : nil)]
  end

  private

  def current_widget
    params[:widget_id].to_i
  end
end
";

const ASSERTIONS: &str = r#"
require_relative "app/controllers/parts_controller"

paths = PartsController.new.part_paths(1)
raise paths.inspect unless paths == ["/widgets/1/parts", "/widgets/1/parts"]
tops = PartsController.new.top_paths(3)
raise tops.inspect unless tops == ["/top/3", "/top/3", "/top"]
maybes = [PartsController.new.maybe_top_paths(3, true), PartsController.new.maybe_top_paths(3, false)]
raise maybes.inspect unless maybes == [["/top/3", "/top/3"], ["/top", "/top"]]
puts "part paths passed"
"#;

fn example() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema.define do\nend\n")
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("app/controllers/parts_controller.rb", CONTROLLER)
        .write(
            "config/routes.rb",
            "Rails.application.routes.draw do\n  resources :widgets, only: [] do\n    resources :parts, only: :index\n  end\n  get \"top(/:length)\", to: \"parts#show\", as: :top\nend\n",
        )
}

#[test]
fn an_integer_in_a_string_segment_is_stringified_at_the_call_site() {
    let (emitted, _) = example().emit(BuildTarget::Ruby);
    let helpers = std::fs::read_to_string(emitted.join("sig/app/route_helpers.rbs"))
        .or_else(|_| std::fs::read_to_string(emitted.join("app/route_helpers.rbs")))
        .expect("read the emitted helper signatures");
    assert!(helpers.contains("def self.widget_parts_path: (String widget_id) -> String"), "{helpers}");
    let controller = std::fs::read_to_string(emitted.join("app/controllers/parts_controller.rb")).expect("read the emitted controller");
    assert!(
        controller.contains("[RouteHelpers.widget_parts_path(widget_id.to_s), RouteHelpers.widget_parts_path(widget_id.to_s)]"),
        "{controller}"
    );
    // An optional segment (`/top(/:length)`, CodeRabbit on #1473) is
    // `String | nil`: an Integer there is bound once, and only a value
    // that isn't nil becomes its `to_s`.
    assert!(helpers.contains("def self.top_path: (?String? length) -> String"), "{helpers}");
    assert!(controller.contains("[RouteHelpers.top_path((route_segment_0 = length\n"), "{controller}");
    assert!(controller.contains("route_segment_0.nil?") && controller.contains("route_segment_0.to_s"), "{controller}");
    assert!(controller.contains("RouteHelpers.top_path(length.to_s), RouteHelpers.top_path]"), "{controller}");
    // An `Integer | nil` expression (CodeRabbit on #1473) is bound the same way.
    assert!(controller.contains(" = (length if flag)\n"), "{controller}");

}

/// The issue's own shape (roundhouse#1468): in a view, a String call and
/// a foreign key read off a record. The view body reaches emit with the
/// foreign key untyped, so the record's integer column is what says so.
#[test]
fn a_foreign_key_in_a_view_is_stringified_at_the_call_site() {
    let (emitted, _) = emit_and_run::real_blog()
        .edit(
            "app/views/articles/show.html.erb",
            "py-2 rounded\" %>\n<% end %>\n",
            "py-2 rounded\" %>\n<% end %>\n<%= link_to \"Comments\", article_comments_path(@article.id.to_s) %>\n",
        )
        .edit(
            "app/views/comments/_comment.html.erb",
            "</div>\n",
            "</div>\n<%= link_to \"All comments\", article_comments_path(comment.article_id) %>\n",
        )
        .emit(BuildTarget::Spinel);
    let helpers = std::fs::read_to_string(emitted.join("app/route_helpers.rbs")).expect("read the emitted helper signatures");
    assert!(helpers.contains("def self.article_comments_path: (String article_id) -> String"), "{helpers}");
    let partial = std::fs::read_to_string(emitted.join("app/views/comments/_comment.rb")).expect("read the emitted partial");
    assert!(partial.contains("RouteHelpers.article_comments_path(comment.article_id.to_s)"), "{partial}");
}

/// A record's integer column under an optional segment can be nil
/// whatever the schema says: a nullable column, a new record's primary
/// key, a reader the model overrides (CodeRabbit on #1473). Each is bound
/// once and stays `nil` when it is, so the segment is dropped as Rails
/// does (`top_path(nil)` is "/top"); otherwise it's its `to_s`. The Ruby
/// run leaves `note.id` out: an unsaved record's id is `0` in the emitted
/// runtime, where Rails has `nil`, which is a separate gap.
fn notes_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"notes\", force: :cascade do |t|\n    t.integer \"rank\"\n    t.integer \"position\", null: false\n    t.integer \"score\", null: false\n  end\nend\n",
        )
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n")
        .write("app/models/note.rb", "class Note < ApplicationRecord\n  def score\n    nil\n  end\nend\n")
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write(
            "app/controllers/notes_controller.rb",
            "class NotesController < ApplicationController\n  def show\n    render json: note_paths(Note.find(params[:id]))\n  end\n\n  def note_paths(note)\n    [top_path(note.rank), top_path(note.position), top_path(note.score), top_path(\"x\")]\n  end\n\n  def id_path(note)\n    top_path(note.id)\n  end\nend\n",
        )
        .write("config/routes.rb", "Rails.application.routes.draw do\n  get \"top(/:length)\", to: \"notes#show\", as: :top\nend\n")
}

#[test]
fn a_column_under_an_optional_segment_keeps_nil() {
    let (emitted, _) = notes_app().emit(BuildTarget::Ruby);
    let controller = std::fs::read_to_string(emitted.join("app/controllers/notes_controller.rb")).expect("read the emitted controller");
    for column in ["rank", "position", "id", "score"] {
        assert!(controller.contains(&format!(" = note.{column}\n")), "{column}: {controller}");
    }
    assert!(controller.contains("RouteHelpers.top_path(\"x\")"), "{controller}");
}

#[test]
fn a_column_under_an_optional_segment_keeps_nil_in_ruby() {
    let script = r#"
require_relative "app/controllers/notes_controller"
paths = NotesController.new.note_paths(Note.new(position: 4))
raise paths.inspect unless paths == ["/top", "/top/4", "/top", "/top/x"]
puts "note paths passed"
"#;
    let run = notes_app().run_ruby(script);
    run.assert_passes();
    assert!(run.stdout.contains("note paths passed"), "{}", run.stdout);
}

/// The binding takes a name the method doesn't already use (CodeRabbit on
/// #1473): a method with its own `route_segment_0` keeps it. This app's
/// only optional-segment call is that method's, so its binding would be
/// the first `route_segment_0`.
fn own_local_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema.define do\nend\n")
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write(
            "app/controllers/tops_controller.rb",
            "class TopsController < ApplicationController\n  def show\n    render json: own_local_top_path(params[:length].to_i)\n  end\n\n  def own_local_top_path(length)\n    route_segment_0 = \"kept\"\n    [top_path(length), top_path(length.to_s), route_segment_0]\n  end\nend\n",
        )
        .write("config/routes.rb", "Rails.application.routes.draw do\n  get \"top(/:length)\", to: \"tops#show\", as: :top\nend\n")
}

#[test]
fn the_binding_does_not_take_a_local_the_method_uses() {
    let (emitted, _) = own_local_app().emit(BuildTarget::Ruby);
    let controller = std::fs::read_to_string(emitted.join("app/controllers/tops_controller.rb")).expect("read the emitted controller");
    assert!(!controller.contains("(route_segment_0 = length"), "{controller}");
    let script = r#"
require_relative "app/controllers/tops_controller"
own = TopsController.new.own_local_top_path(3)
raise own.inspect unless own == ["/top/3", "/top/3", "kept"]
puts "own local passed"
"#;
    let run = own_local_app().run_ruby(script);
    run.assert_passes();
    assert!(run.stdout.contains("own local passed"), "{}", run.stdout);
}

#[test]
fn an_integer_in_a_string_segment_runs_in_ruby() {
    example().run_ruby(ASSERTIONS).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_integer_in_a_string_segment_runs_on_spinel_with_rbs() {
    example().run_spinel_with_rbs(ASSERTIONS).assert_passes();
}

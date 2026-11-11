//! A serializer that reads `scope`, or `current_user` (the name the
//! default `serialization_scope :current_user` gives it), renders with
//! the scope its controller passes, as active_model_serializers 0.10.16
//! does on Rails 8.1.4: the controller's `current_user` under the default
//! scope, nil without one, and nil under `serialization_scope nil`.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write(
            "app/serializers/viewer_serializer.rb",
            r#"class ViewerSerializer < ActiveModel::Serializer
  attributes :id, :title
  attribute :viewer, if: :current_user?
  attribute(:scoped) { scope.nil? ? "none" : scope }

  def viewer
    current_user.upcase
  end

  def current_user?
    !current_user.nil?
  end
end
"#,
        )
        .write(
            "app/serializers/scope_only_serializer.rb",
            "class ScopeOnlySerializer < ActiveModel::Serializer\n  attributes :id\n  attribute(:scoped) { scope.nil? ? \"none\" : scope }\nend\n",
        )
        .write(
            "app/controllers/viewer_json_controller.rb",
            r#"class ViewerJsonController < ApplicationController
  def show
    render json: Article.find(params[:id]), serializer: ViewerSerializer
  end

  def index
    render json: Article.order(:id), each_serializer: ViewerSerializer
  end

  def scope_only
    render json: Article.find(params[:id]), serializer: ScopeOnlySerializer
  end

  private

  def current_user
    params[:who]
  end
end
"#,
        )
        .write(
            "app/controllers/anonymous_json_controller.rb",
            r#"class AnonymousJsonController < ApplicationController
  def show
    render json: Article.find(params[:id]), serializer: ScopeOnlySerializer
  end

  def viewer
    render json: Article.find(params[:id]), serializer: ViewerSerializer
  end
end
"#,
        )
        .write(
            "app/controllers/quiet_json_controller.rb",
            r#"class QuietJsonController < ApplicationController
  serialization_scope nil

  def show
    render json: Article.find(params[:id]), serializer: ScopeOnlySerializer
  end

  private

  def current_user
    "ignored"
  end
end
"#,
        )
        .edit(
            "config/routes.rb",
            "  resources :articles do",
            "  get \"viewer\" => \"viewer_json#index\"\n  get \"viewer/:id/scope_only\" => \"viewer_json#scope_only\"\n  get \"viewer/:id\" => \"viewer_json#show\"\n  get \"anonymous/:id/viewer\" => \"anonymous_json#viewer\"\n  get \"anonymous/:id\" => \"anonymous_json#show\"\n  get \"quiet/:id\" => \"quiet_json#show\"\n  resources :articles do",
        )
}

/// The bodies are active_model_serializers 0.10.16's for the same
/// controllers and records.
const SCRIPT: &str = r##"
%w[viewer_json anonymous_json quiet_json].each { |c| require_relative "app/controllers/#{c}_controller" }
a = Article.create!(title: "Hello", body: "A long body")
b = Article.create!(title: "World", body: "Another body")
call = ->(controller, action, params) { c = controller.new; c.params = params; c.process_action(action); c.body }
check = ->(label, got, want) { raise "#{label}: #{got.inspect}, want #{want.inspect}" unless got == want }
ida, idb = a.id.to_s, b.id.to_s
check.("default scope, signed in", call.(ViewerJsonController, :show, { "id" => ida, "who" => "ann" }),
  '{"id":' + ida + ',"title":"Hello","viewer":"ANN","scoped":"ann"}')
check.("default scope, signed out", call.(ViewerJsonController, :show, { "id" => ida }),
  '{"id":' + ida + ',"title":"Hello","scoped":"none"}')
check.("each_serializer", call.(ViewerJsonController, :index, { "who" => "ann" }),
  '[{"id":' + ida + ',"title":"Hello","viewer":"ANN","scoped":"ann"},{"id":' + idb + ',"title":"World","viewer":"ANN","scoped":"ann"}]')
check.("scope only, signed in", call.(ViewerJsonController, :scope_only, { "id" => ida, "who" => "ann" }),
  '{"id":' + ida + ',"scoped":"ann"}')
check.("no current_user, scope only", call.(AnonymousJsonController, :show, { "id" => ida }),
  '{"id":' + ida + ',"scoped":"none"}')
check.("no current_user, viewer", call.(AnonymousJsonController, :viewer, { "id" => ida }),
  '{"id":' + ida + ',"title":"Hello","scoped":"none"}')
check.("serialization_scope nil", call.(QuietJsonController, :show, { "id" => ida }),
  '{"id":' + ida + ',"scoped":"none"}')
puts "serializer scope OK"
"##;

#[test]
fn a_serializer_reads_the_scope_its_controller_passes() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("serializer scope OK"), "{}", run.stdout);
}

#[test]
fn a_serializer_reading_its_scope_emits_for_spinel() {
    let (_emitted, errors) = app().emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
}

use super::data_paths_contract as contract;

/// A typed_store attribute validates through its reader, the constructor
/// calls an app's own column writer only for a key it was given,
/// `first_or_initialize` carries the where-conditions, `order` takes an
/// Array, and a framework setting the app reads answers the running
/// environment's value; each as Rails 8.1 answers it.
#[test]
fn lobsters_data_path_constructs_run_after_emission() {
    let run = contract::overlay().run_ruby(contract::ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("data paths contract passed"), "{}", run.stdout);
}

const TEST: &str = r#"require "test_helper"

class BannerControllerTest < ActionDispatch::IntegrationTest
  test "a computed render local reaches the partial and links by to_param" do
    get articles_url
    assert_response :success
    first = Article.order(:id).first
    assert_select "p#banner a[href=?]", "/articles/a#{first.id}", text: first.title
  end
end
"#;

/// `@above = {partial:, locals: {banner_article: <a call>}}` rendered
/// with `render @above`: the computed local reaches the partial (it was
/// dropped, so lobsters' user stories page raised on nil), and the
/// record links by the model's `to_param`, not its id.
#[test]
fn a_computed_dynamic_render_local_reaches_the_partial() {
    contract::overlay()
        .write("test/controllers/banner_controller_test.rb", TEST)
        .run_test("test/controllers/banner_controller_test.rb")
        .assert_passes();
}

/// ActionMailer's railtie adds `protocol: "https"` to the mailer's
/// `default_url_options` where `force_ssl` is on, so a class that builds
/// URLs from that setting links over https in production.
#[test]
fn a_force_ssl_environment_adds_the_mailer_https_protocol() {
    let (emitted, errors) = contract::overlay()
        .edit(
            "config/environments/production.rb",
            "  # config.force_ssl = true\n",
            "  config.force_ssl = true\n",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let application = std::fs::read_to_string(emitted.join("config/application.rb")).expect("application.rb");
    assert!(
        application.contains("when \"production\"\n        { protocol: \"https\" }.merge({ host: \"example.com\" })"),
        "{application}"
    );
    assert!(application.contains("when \"development\"\n        { host: \"localhost\", port: 3000 }"), "{application}");
}

/// A framework setting only a controller reads is lifted too: the reads
/// are collected after the controllers and views are ingested.
#[test]
fn a_framework_setting_a_controller_reads_is_lifted() {
    let (emitted, errors) = super::emit_and_run::real_blog()
        .edit(
            "app/controllers/articles_controller.rb",
            "    @articles = Article.includes(:comments).order(created_at: :desc)\n",
            "    @articles = Article.includes(:comments).order(created_at: :desc)\n    @mailer_host = Rails.application.config.action_mailer.default_url_options[:host]\n",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let application = std::fs::read_to_string(emitted.join("config/application.rb")).expect("application.rb");
    assert!(application.contains("def action_mailer_default_url_options"), "{application}");
}

/// `form_with model: @featured`, where `@featured` holds an Article whose
/// model overrides `to_param`: the action's member is that `to_param`,
/// read off the record's class rather than the local's name.
#[test]
fn a_form_for_a_typed_record_posts_to_its_to_param() {
    let (emitted, errors) = contract::overlay()
        .edit("app/controllers/articles_controller.rb", "  def show\n  end\n", "  def show\n    @featured = @article\n  end\n")
        .edit(
            "app/views/articles/show.html.erb",
            "<% content_for :title, \"Showing article\" %>\n",
            "<% content_for :title, \"Showing article\" %>\n<%= form_with model: @featured do |f| %><% end %>\n",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let view = std::fs::read_to_string(emitted.join("app/views/articles/show.rb")).expect("show view");
    assert!(view.contains("article_path(featured.to_param)"), "{view}");
}

/// `force_ssl` in an environment that does not assign the mailer's
/// options itself still adds the railtie's https to the value it
/// inherits from application.rb.
#[test]
fn force_ssl_applies_to_an_inherited_mailer_setting() {
    let (emitted, errors) = contract::overlay()
        .edit(
            "config/application.rb",
            "    config.load_defaults 8.1\n",
            "    config.load_defaults 8.1\n    config.action_mailer.default_url_options = { host: \"base.example\" }\n",
        )
        .edit(
            "config/environments/production.rb",
            "  config.action_mailer.default_url_options = { host: \"example.com\" }\n",
            "",
        )
        .edit(
            "config/environments/production.rb",
            "  # config.force_ssl = true\n",
            "  config.force_ssl = true\n",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let application = std::fs::read_to_string(emitted.join("config/application.rb")).expect("application.rb");
    assert!(
        application.contains("when \"production\"\n        { protocol: \"https\" }.merge({ host: \"base.example\" })"),
        "{application}"
    );
}

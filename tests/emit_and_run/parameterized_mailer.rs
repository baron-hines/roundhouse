//! `TitleMailer.with(to: …).titled(self)` is a parameterized mailer: the
//! action reads `params`, and `with` hands its argument on to `titled`
//! as `TitleMailer.titled(self)` does. The emitted runtime had neither
//! `with` nor `params`, so `check` was clean while the program raised
//! NoMethodError, and the untyped `with` left `@article` unknown in the
//! template (mastodon's admin mailers).

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/mailers/title_mailer.rb", "class TitleMailer < ApplicationMailer\n  def titled(article)\n    @article = article\n    mail(to: params[:to], subject: \"Titled\")\n  end\nend\n")
        .write("app/views/title_mailer/titled.text.erb", "Title: <%= @article.title.upcase %>\n")
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true\n\n  def announce\n    m = TitleMailer.with(to: \"reader@example.com\").titled(self).deliver_now\n    [m.to, m.body.to_s]\n  end\n",
        )
}

const ASSERTIONS: &str = r##"got = Article.new(title: "x").announce
raise got.inspect unless got == [["reader@example.com"], "Title: X\n\n"]
puts "parameterized mailer passed"
"##;

#[test]
fn a_parameterized_action_types_its_argument() {
    let errors = app().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
    app().run_ruby(ASSERTIONS).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_parameterized_action_types_its_argument_on_spinel() {
    app().run_spinel(ASSERTIONS).assert_passes();
}

/// Rails' `params` is an empty hash until `with` gives it one.
#[test]
fn an_unparameterized_action_reads_empty_params() {
    let app = emit_and_run::real_blog()
        .write("app/mailers/title_mailer.rb", "class TitleMailer < ApplicationMailer\n  def titled(article)\n    @article = article\n    mail(to: params.fetch(:to, \"default@example.com\"), subject: \"Titled\")\n  end\nend\n")
        .write("app/views/title_mailer/titled.text.erb", "Title: <%= @article.title %>\n")
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true\n\n  def announce\n    TitleMailer.titled(self).deliver_now.to\n  end\n",
        );
    app.run_ruby("got = Article.new(title: \"x\").announce\nraise got.inspect unless got == [\"default@example.com\"]\nputs \"empty params passed\"\n")
        .assert_passes();
}

/// maybe's `EmailConfirmationMailer#confirmation_email`: `t(".cta")` in a mailer action reads
/// `email_confirmation_mailer.confirmation_email.cta` (AbstractController::Translation's lazy key, the action it
/// is in), and a bare `article_url(…)` is the routes' url helper Rails' mailers include, so both ivars are typed in
/// the template and render what Rails renders.
fn lazy_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/mailers/title_mailer.rb", "class TitleMailer < ApplicationMailer\n  default from: \"from@example.com\"\n\n  def titled(article)\n    @cta = t(\".cta\")\n    @fallback = t(\".fallback\")\n    @link = article_url(article, host: \"example.com\")\n    mail(to: \"reader@example.com\", subject: \"Titled\")\n  end\nend\n")
        .write("app/views/title_mailer/titled.text.erb", "<%= @cta.upcase %> <%= @fallback %> <%= @link %>\n")
        .write("config/locales/mailers.en.yml", "en:\n  title_mailer:\n    fallback: Shared\n    titled:\n      cta: Read it\n")
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true\n\n  def announce\n    TitleMailer.titled(self).deliver_now.body.to_s\n  end\n",
        )
}

#[test]
fn a_mailer_actions_lazy_translation_and_url_helper_are_typed() {
    let errors = lazy_app().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
    lazy_app()
        .run_ruby("a = Article.create!(title: \"Hello\", body: \"A body long enough for validation.\")\ngot = a.announce\nraise got.inspect unless got == \"READ IT Shared http://example.com/articles/#{a.id}\\n\\n\"\nputs \"lazy mailer passed\"\n")
        .assert_passes();
}

/// The same `t(".cta")` in a mailer's private helper reads the lazy key of whichever action called it, which the
/// compile cannot name: refused, not emitted to raise.
#[test]
fn a_mailer_helpers_lazy_translation_is_refused() {
    let errors = lazy_app()
        .write("app/mailers/title_mailer.rb", "class TitleMailer < ApplicationMailer\n  default from: \"from@example.com\"\n\n  def titled(article)\n    @cta = cta_text\n    mail(to: \"reader@example.com\", subject: \"Titled\")\n  end\n\n  private\n\n  def cta_text\n    t(\".cta\")\n  end\nend\n")
        .write("app/views/title_mailer/titled.text.erb", "<%= @cta %>\n")
        .check_errors();
    assert!(errors.iter().any(|e| e.contains("mailer translation outside an action")), "{errors:#?}");
}

/// A key known only when the action runs may be a lazy `.key`, whose mailer-level fallback the run-time lookup does
/// not carry: refused.
#[test]
fn a_mailer_actions_computed_key_is_refused() {
    let errors = lazy_app()
        .write("app/mailers/title_mailer.rb", "class TitleMailer < ApplicationMailer\n  default from: \"from@example.com\"\n\n  def titled(article)\n    key = article.title.present? ? \".cta\" : \".fallback\"\n    @cta = t(key)\n    mail(to: \"reader@example.com\", subject: \"Titled\")\n  end\nend\n")
        .write("app/views/title_mailer/titled.text.erb", "<%= @cta %>\n")
        .check_errors();
    assert!(errors.iter().any(|e| e.contains("a mailer action's key computed when it runs")), "{errors:#?}");
}

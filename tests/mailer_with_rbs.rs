//! A mailer's `with` keeps the type its signature declares (roundhouse#1469).
//! `with` is declared to return the mailer, and its body returns what
//! `new` built rather than the `self` the runtime's `_with_params`
//! answers, which Spinel types as `ActionMailer::Base`. Under `--rbs .`
//! that was a contradicted return seed, and the build stopped.
//!
//! actionmailer 8.1.4 returns a proxy from `with`; the mail it builds
//! is the same: `NoticeMailer.with(to: "a@example.com").hello` is to
//! `a@example.com`, subject "hello".

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

use roundhouse::project::BuildTarget;

const NOTICE_MAILER: &str = "class NoticeMailer < ApplicationMailer\n  def hello\n    mail(to: params[:to], subject: \"hello\", body: \"hi\")\n  end\nend\n";

// App code that calls `with`: the wrapper is built for a mailer the app
// parameterizes.
const NOTICES: &str = "class Notices\n  def self.with(params)\n    NoticeMailer.with(params)\n  end\nend\n";

const SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
m = Notices.with(to: "a@example.com").hello.deliver_now
check("with's params reach the action", [m.to, m.subject], [["a@example.com"], "hello"])
puts "mailer with passed"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/mailers/notice_mailer.rb", NOTICE_MAILER)
        .write("app/models/notices.rb", NOTICES)
}

#[test]
fn with_returns_the_mailer_new_built() {
    let (emitted, _) = app().emit(BuildTarget::Spinel);
    let mailer = std::fs::read_to_string(emitted.join("app/models/notice_mailer.rb")).expect("read the emitted mailer");
    assert!(
        mailer.contains("  def self.with(params)\n    mailer = new\n    mailer._with_params(params)\n    mailer\n  end\n"),
        "{mailer}"
    );
}

#[test]
fn a_parameterized_mailer_runs_on_ruby() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("mailer with passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_parameterized_mailer_runs_on_spinel_with_rbs() {
    let run = app().run_spinel_with_rbs(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("mailer with passed"), "{}", run.stdout);
}

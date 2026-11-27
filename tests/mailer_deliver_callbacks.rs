//! A mailer's `before_deliver` / `around_deliver` / `after_deliver`.
//! actionmailer 8.1.4 (oracle `test_app/oracle.rb`, which runs these same
//! classes and script) runs them in `deliver_now`, around the delivery,
//! nested as declared and parent first, with the mail as `message`. A
//! `throw :abort` in one is caught: the mail isn't sent. An action that
//! builds no mail answers a `NullMail`, whose delivery runs the callbacks
//! and sends nothing.
//!
//! Each callback was reported and dropped, and `deliver_now` on an action
//! that built no mail raised NoMethodError.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const LOG: &str = "class DeliverLog\n  def self.entries\n    @entries ||= []\n  end\n\n  def self.add(entry)\n    entries << entry\n  end\n\n  def self.clear\n    @entries = []\n  end\nend\n";
const APPLICATION_MAILER: &str = "class ApplicationMailer < ActionMailer::Base\n  default from: \"from@example.com\"\n  layout \"mailer\"\n  after_deliver { DeliverLog.add(\"base after_deliver\") }\nend\n";
const DELIVER_MAILER: &str = "class DeliverMailer < ApplicationMailer\n  before_deliver :check\n  around_deliver :wrap\n  after_deliver :note\n\n  def hello\n    DeliverLog.add(\"action\")\n    mail(to: \"a@example.com\", subject: \"Hello\", body: \"h\")\n  end\n\n  def nothing\n    DeliverLog.add(\"action\")\n  end\n\n  private\n\n  def check\n    DeliverLog.add(\"before_deliver \" + message.subject.to_s)\n    throw :abort if params[:halt]\n  end\n\n  def wrap\n    DeliverLog.add(\"around_in\")\n    yield\n    DeliverLog.add(\"around_out\")\n  end\n\n  def note\n    DeliverLog.add(\"after_deliver\")\n  end\nend\n";
// App code that calls `with`, as a parameterized mailer's caller does.
const DELIVERS: &str = "class Delivers\n  def self.with(params)\n    DeliverMailer.with(params)\n  end\nend\n";

const SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
DeliverLog.clear
ActionMailer::Base.deliveries.clear
built = DeliverMailer.hello
DeliverLog.add("built")
built.deliver_now
# The action runs at `DeliverMailer.hello` here (eager) and at delivery in
# Rails: the markers are left out of the order.
order = DeliverLog.entries.reject { |e| e == "built" || e == "action" }
check("delivery callbacks, nested as declared, parent first", order, ["before_deliver Hello", "around_in", "after_deliver", "around_out", "base after_deliver"])
check("delivered", ActionMailer::Base.deliveries.size, 1)
DeliverLog.clear
ActionMailer::Base.deliveries.clear
Delivers.with(halt: true).hello.deliver_now
check("throw :abort in before_deliver stops the delivery", [DeliverLog.entries, ActionMailer::Base.deliveries.size], [["action", "before_deliver Hello"], 0])
DeliverLog.clear
ActionMailer::Base.deliveries.clear
DeliverMailer.nothing.deliver_now
check("an action that builds no mail: callbacks run, nothing is sent", [DeliverLog.entries, ActionMailer::Base.deliveries.size], [["action", "before_deliver ", "around_in", "after_deliver", "around_out", "base after_deliver"], 0])
puts "deliver callbacks passed"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/models/deliver_log.rb", LOG)
        .write("app/mailers/application_mailer.rb", APPLICATION_MAILER)
        .write("app/mailers/deliver_mailer.rb", DELIVER_MAILER)
        .write("app/models/delivers.rb", DELIVERS)
}

#[test]
fn delivery_callbacks_run_around_the_delivery_on_ruby() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("deliver callbacks passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn delivery_callbacks_run_around_the_delivery_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("deliver callbacks passed"), "{}", run.stdout);
}

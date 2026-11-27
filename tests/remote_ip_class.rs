//! An app that reopens `class ActionDispatch::RemoteIp` (a module of its
//! own inside, as `lib/action_dispatch/` extensions do). Rails'
//! RemoteIp is the middleware class; the runtime declared a module to
//! hold `IpSpoofAttackError`, so the emitted tree raised
//! `RemoteIp is not a class (TypeError)` at boot. Its oracle:
//!
//!     ruby tests/remote_ip_class/oracle.rb

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const REMOTE_IP: &str = include_str!("remote_ip_class/remote_ip.rb");
const SCRIPT: &str = include_str!("remote_ip_class/script.rb");

#[test]
fn an_app_reopening_remote_ip_as_a_class_boots_on_ruby() {
    let run = emit_and_run::real_blog().write("lib/action_dispatch/remote_ip.rb", REMOTE_IP).run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("remote ip passed"), "{}", run.stdout);
}

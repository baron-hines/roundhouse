//! An html_safe String plus a `nil` operand is the String, still
//! html_safe, and plus an unsafe String escapes it, as Rails'
//! `SafeBuffer#+` answers them. Ignored as a fork spec until the compiler
//! does; the expected values are ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/caption.rb", r##"class Caption
  def self.join(head, tail)
    head.html_safe + tail
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    with_nil = Caption.join("<b>hi</b>", nil)
    # Rails: "<b>hi</b>", true
    check("plus nil", with_nil == "<b>hi</b>" && with_nil.html_safe?)
    # Rails: "<b>hi</b>&lt;i&gt;", true
    escaped = Caption.join("<b>hi</b>", "<i>")
    check("plus unsafe", escaped == "<b>hi</b>&lt;i&gt;" && escaped.html_safe?)
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: ruby semantics"]
fn an_html_safe_string_plus_nil_is_the_string() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn an_html_safe_string_plus_nil_is_the_string_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

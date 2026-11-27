//! A Symbol key read from a value that may be an indifferent hash, a
//! plain Hash or another object answering `[]` stays a Symbol, so each
//! receiver answers its own `[]` for it, as Ruby sends it. Ignored as a
//! fork spec until the compiler does; the expected values are
//! ActiveRecord 8.1.4's answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/settings_reader.rb", r##"class SettingsReader
  def self.mode_of(source)
    source[:mode]
  end
end
"##)
        .write("app/models/symbol_box.rb", r##"class SymbolBox
  def [](key)
    key.is_a?(Symbol) ? "symbol #{key}" : "other"
  end
end
"##)
        .write("app/models/probe.rb", r##"module Probe
  def self.check(label, ok)
    raise "#{label} differs from Rails" unless ok
  end

  def self.run
    # Rails: "fast", "slow", "symbol mode"
    check("plain Hash", SettingsReader.mode_of({ mode: "fast" }) == "fast")
    check("indifferent hash", SettingsReader.mode_of({ "mode" => "slow" }.with_indifferent_access) == "slow")
    check("other object", SettingsReader.mode_of(SymbolBox.new) == "symbol mode")
  end
end
"##)
}

const SCRIPT: &str = "Probe.run\nputs \"probe passed\"\n";

#[test]
#[ignore = "fork spec: ruby semantics"]
fn a_symbol_key_stays_a_symbol_for_any_receiver() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

#[test]
#[ignore = "fork spec: ruby semantics"]
fn a_symbol_key_stays_a_symbol_for_any_receiver_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("probe passed"), "{}", run.stdout);
}

//! `symbolize_keys` and `stringify_keys` on a parameter or instance
//! variable with no static type. A plain Hash converts through the
//! shared helper; any other value keeps its own method, and nil raises
//! NoMethodError, as in Rails.

#[path = "support/prerequisite.rs"]
mod prerequisite;
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;

const OPTIONS: &str = r#"class SavedOptions
  def self.symbolized(saved)
    { note: nil }.merge(saved.symbolize_keys)
  end

  def self.stringified(saved)
    { "note" => nil }.merge(saved.stringify_keys)
  end

  def initialize(saved)
    @saved = saved
  end

  def symbolized
    @saved.symbolize_keys
  end
end
"#;

const KEY_SOURCE: &str = r#"class KeySource
  def symbolize_keys
    { name: "custom" }
  end

  def stringify_keys
    { "name" => "custom" }
  end
end
"#;

const CASES: &str = r##"
[{ "name" => "first" }, { name: "second" }, {}, KeySource.new, nil].each do |value|
  begin
    fields = SavedOptions.symbolized(value)
    puts "#{fields.keys.map(&:inspect).join(",")}=#{fields[:name]}"
  rescue NoMethodError => e
    puts e.message
  end
  begin
    fields = SavedOptions.stringified(value)
    puts "#{fields.keys.join(",")}=#{fields["name"]}"
  rescue NoMethodError => e
    puts e.message
  end
end
puts SavedOptions.new({ "name" => "first" }).symbolized.keys.map(&:inspect).join(",")
puts SavedOptions.new(KeySource.new).symbolized[:name]
"##;

const EXPECTED: &str = ":note,:name=first
note,name=first
:note,:name=second
note,name=second
:note=
note=
:note,:name=custom
note,name=custom
undefined method 'symbolize_keys' for nil
undefined method 'stringify_keys' for nil
:name
custom
";

fn overlay() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/saved_options.rb", OPTIONS)
        .write("app/models/key_source.rb", KEY_SOURCE)
}

#[test]
fn rails_key_conversion_oracle() {
    let oracle = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/rails_oracle.rb");
    let script = format!(
        "require {oracle:?}\n\
         unless RailsOracle.available?(\"activesupport\")\n\
           warn \"skipping: #{{RailsOracle.label}} activesupport is not installed\"\n\
           exit 77\n\
         end\n\
         RailsOracle.activate(\"activesupport\")\n\
         require 'active_support/core_ext/hash/keys'\n{OPTIONS}\n{KEY_SOURCE}\n{CASES}"
    );
    let output = std::process::Command::new("ruby").args(["-e", &script]).output().expect("Rails oracle");
    if output.status.code() == Some(77) {
        prerequisite::missing(String::from_utf8_lossy(&output.stderr).trim());
        return;
    }
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED);
}

#[test]
fn emitted_conversions_match_rails() {
    let run = overlay().run_ruby(CASES);
    run.assert_passes();
    assert_eq!(run.stdout, EXPECTED);
    let source = std::fs::read_to_string(run.emitted.join("app/models/saved_options.rb")).unwrap();
    let source = source.split_whitespace().collect::<Vec<_>>().join(" ");
    for guarded in [
        "merge(if saved.instance_of?(Hash) ActiveSupport.symbolize_keys(saved) else saved.symbolize_keys end)",
        "merge(if saved.instance_of?(Hash) ActiveSupport.stringify_keys(saved) else saved.stringify_keys end)",
        "if @saved.instance_of?(Hash) ActiveSupport.symbolize_keys(@saved) else @saved.symbolize_keys end",
    ] {
        assert!(source.contains(guarded), "{guarded}\n{source}");
    }
}

/// The guard reads its receiver twice, and stands in for a conversion
/// that answers a new Hash, so other forms keep the call as written.
#[test]
fn other_untyped_forms_keep_their_calls() {
    let source = r#"class OtherForms
  def self.in_place(saved)
    saved.symbolize_keys!
  end

  def self.nested(saved)
    saved.deep_symbolize_keys
  end

  def self.computed(source)
    source.fields.symbolize_keys
  end

  def self.typed
    { "name" => 1 }.symbolize_keys
  end
end
"#;
    let (tree, errors) = emit_and_run::empty_app().write("app/models/other_forms.rb", source).emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:?}");
    let emitted = std::fs::read_to_string(tree.join("app/models/other_forms.rb")).unwrap();
    assert!(!emitted.contains("instance_of?"), "{emitted}");
    for kept in [
        "saved.symbolize_keys!",
        "saved.deep_symbolize_keys",
        "source.fields.symbolize_keys",
        "ActiveSupport.symbolize_keys({ \"name\" => 1 })",
    ] {
        assert!(emitted.contains(kept), "{kept}\n{emitted}");
    }
}

/// Only the probe and the runtime file it calls, built with the
/// generated signatures as emitted.
#[test]
#[ignore = "requires the Spinel toolchain"]
fn native_conversions_match_rails_with_generated_signatures() {
    let (tree, errors) = overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    let signatures = std::fs::read_to_string(tree.join("app/models/saved_options.rbs")).unwrap();
    assert!(signatures.contains("def self.symbolized: (untyped saved)"), "{signatures}");
    std::fs::write(
        tree.join("contract.rb"),
        format!(
            "require_relative \"runtime/active_support_ext\"\nrequire_relative \"app/models/key_source\"\nrequire_relative \"app/models/saved_options\"\n{CASES}"
        ),
    )
    .unwrap();
    let spinel = std::env::var("SPINEL").unwrap_or_else(|_| "spinel".into());
    // The app's signatures alone, then every emitted sidecar, as the shipped Makefile builds.
    for signatures in ["app/models", "."] {
        let built = std::process::Command::new(&spinel)
            .args(["--strict-require", "--rbs", signatures, "contract.rb", "-o", "contract"])
            .current_dir(&tree)
            .output()
            .unwrap();
        assert!(built.status.success(), "--rbs {signatures}: {}", String::from_utf8_lossy(&built.stderr));
        let run = std::process::Command::new(tree.join("contract")).current_dir(&tree).output().unwrap();
        assert!(run.status.success(), "--rbs {signatures}: {}", String::from_utf8_lossy(&run.stderr));
        assert_eq!(String::from_utf8_lossy(&run.stdout), EXPECTED, "--rbs {signatures}");
    }
}

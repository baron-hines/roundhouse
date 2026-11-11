//! `with_options` in code that runs, as devise's `modules.rb` writes
//! `Devise.with_options model: true do |d| d.add_module … end`: each call
//! on the block's argument goes to the receiver with the options
//! deep-merged into its trailing Hash, or appended as one, and a nested
//! `with_options` merges into the outer options (`ingest::option_merger`).

use super::emit_and_run;

const REGISTRY: &str = r##"class Registry
  @@entries = []

  def self.add(name, kind: "none", tags: {})
    @@entries << "#{name}:#{kind}:#{tags.keys.map { |k| k.to_s }.sort.join("")}"
  end

  def self.entries
    @@entries.join(",")
  end
end
"##;

const INITIALIZER: &str = r#"Registry.with_options(kind: "plain", tags: { a: 1 }) do |r|
  r.add("one")
  r.add("two", kind: "loud", tags: { b: 2 })
  r.with_options(tags: { c: 3 }) do |s|
    s.add("three")
  end
end
"#;

const SCRIPT: &str = r#"got = Registry.entries
raise "got #{got.inspect}" unless got == "one:plain:a,two:loud:ab,three:plain:ac"
puts "with_options passed"
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/registry.rb", REGISTRY).write("config/initializers/registry.rb", INITIALIZER)
}

#[test]
fn with_options_merges_into_each_call_on_its_argument() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("with_options passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn with_options_merges_into_each_call_on_its_argument_natively() {
    let run = app().run_spinel(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("with_options passed"), "{}", run.stdout);
}

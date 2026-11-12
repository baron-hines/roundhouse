//! `Model.human_attribute_name(attr)` of an attribute only known when it runs (Devise's FailureApp:
//! `scope_class.human_attribute_name(key)`) answers the locale's name for it, as Rails' does, and humanizes one the
//! locale does not name.

use super::emit_and_run;

const LOCALE: &str = "en:\n  activerecord:\n    attributes:\n      article:\n        title: \"Headline\"\n";

const SCRIPT: &str = r#"named = [:title, "body", :created_at].map { |attr| Article.human_attribute_name(attr) }
raise "names #{named.inspect}" unless named == ["Headline", "Body", "Created at"]
puts "dynamic human attribute name passed"
"#;

fn overlay() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("config/locales/en.yml", LOCALE)
}

#[test]
fn a_run_time_attribute_name_reads_the_locale() {
    let run = overlay().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("dynamic human attribute name passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_run_time_attribute_name_reads_the_locale_natively() {
    let run = overlay().run_spinel(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("dynamic human attribute name passed"), "{}", run.stdout);
}

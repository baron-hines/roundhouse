use super::{emit_and_run, string_truncate_words};

#[test]
fn string_truncate_words_runs_in_emitted_ruby() {
    let run = emit_and_run::real_blog()
        .write(
            "app/models/string_truncate_words_probe.rb",
            string_truncate_words::SOURCE,
        )
        .run_ruby(string_truncate_words::ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("String truncate_words emitted contract passed"));
    let emitted = std::fs::read_to_string(run.emitted.join("app/models/string_truncate_words_probe.rb"))
        .expect("emitted probe class");
    assert!(emitted.contains("ActiveSupport.truncate_words("), "{emitted}");
    assert_eq!(
        emitted.matches(".truncate_words(").count(),
        emitted.matches("ActiveSupport.truncate_words(").count(),
        "every String#truncate_words is grounded:\n{emitted}"
    );
}

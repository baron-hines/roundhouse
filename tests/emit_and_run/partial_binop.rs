use super::emit_and_run;

/// An operator only some arms of its operand take (lobsters' search view
/// compares a value that is an Integer or a request String with `>`). The
/// ruby family runs it as written: the arm that takes it answers, the rest
/// raise as Ruby does. Analysis reports a warning; a strict target, which
/// would raise for every arm, reports it.
fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit(
        "app/models/article.rb",
        "  validates :title, presence: true\n",
        "  validates :title, presence: true

  def self.over?(counted)
    value = counted ? 5 : \"five\"
    value > 3
  end
",
    )
}

#[test]
fn the_ruby_family_runs_the_arm_that_takes_it() {
    let (_emitted, errors) = app().emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let run = app().run_ruby(r#"raise "counted" unless Article.over?(true) == true
begin
  Article.over?(false)
  raise "no raise"
rescue ArgumentError => e
  raise "message: #{e.message}" unless e.message == "comparison of String with 3 failed"
end
puts "partial binop passed"
"#);
    run.assert_passes();
    assert!(run.stdout.contains("partial binop passed"));
}

#[test]
fn a_strict_target_reports_the_operator() {
    let (_emitted, errors) = app().emit(roundhouse::project::BuildTarget::Rust);
    assert!(errors.iter().any(|e| e.contains("operator on some of its operands' types not supported (rust)")), "{errors:#?}");
}

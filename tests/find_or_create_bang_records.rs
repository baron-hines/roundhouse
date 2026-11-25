//! Bang find-or-create accepts literal and variable conditions.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "config/routes.rb",
            "Rails.application.routes.draw do\nend\n",
        )
        .write(
            "db/schema.rb",
            include_str!("find_or_create_bang_records/schema.rb"),
        )
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n",
        )
        .write(
            "app/models/user.rb",
            "class User < ApplicationRecord\nend\n",
        )
        .write(
            "app/models/article.rb",
            "class Article < ApplicationRecord\nend\n",
        )
        .write(
            "app/models/follow.rb",
            include_str!("find_or_create_bang_records/follow.rb"),
        )
        .write(
            "app/models/favorite.rb",
            include_str!("find_or_create_bang_records/favorite.rb"),
        )
}

#[test]
fn pure_read_conditions_build_or_find_records_in_single_statement_branches() {
    let run = app().run_ruby(include_str!("find_or_create_bang_records/probe.rb"));
    run.assert_passes();
    assert!(
        run.stdout.contains("bang find-or-create passed"),
        "{}",
        run.stdout
    );
}

#[test]
fn record_conditions_and_validation_failures_match_rails() {
    let oracle = std::process::Command::new("ruby")
        .arg("tests/find_or_create_bang_records/oracle.rb")
        .output()
        .expect("run Rails oracle");
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let run = app().run_ruby(include_str!("find_or_create_bang_records/probe.rb"));
    run.assert_passes();
    assert_eq!(run.stdout, String::from_utf8_lossy(&oracle.stdout));
}

#[test]
fn method_call_conditions_keep_the_unsupported_boundary() {
    let run = app()
        .write(
            "app/models/follow.rb",
            r#"
class Follow < ApplicationRecord
  belongs_to :follower, class_name: "User"
  belongs_to :followed, class_name: "User"
  def self.effectful
    Follow.find_or_create_by!(follower: User.first, followed: User.first)
  end
end
"#,
        )
        .run_ruby("");
    assert!(
        run.errors
            .iter()
            .any(|error| error.contains("scoped find-or-create cannot be lowered safely")),
        "method calls would be evaluated for both lookup and construction: {:?}",
        run.errors
    );
}

fn reviewed_app() -> emit_and_run::Overlay {
    let oracle = std::process::Command::new("ruby")
        .arg("tests/find_or_create_bang_records/review_oracle.rb")
        .output()
        .expect("run Rails review oracle");
    assert!(oracle.status.success(), "{}", String::from_utf8_lossy(&oracle.stderr));
    assert_eq!(String::from_utf8_lossy(&oracle.stdout), concat!(
        "conflicting_keys | false | 1 | 1\n",
        "reversed_keys | false | 1 | 3\n",
        "shared_key | false | 1 | 1\n",
        "block | 3:1 | 3:1\n",
        "existing | true | existing body\n",
        "repair | true | 3:2\n",
        "literal | 2:1\n",
    ));
    app().write("app/models/follow.rb", include_str!("find_or_create_bang_records/review_follow.rb"))
}

#[test]
fn colliding_association_predicates_are_refused_instead_of_collapsed() {
    for method in ["find_or_create_by!", "find_or_create_by"] {
        for target in [roundhouse::project::BuildTarget::Ruby, roundhouse::project::BuildTarget::Spinel] {
            let model = include_str!("find_or_create_bang_records/review_follow.rb")
                .replace("find_or_create_by!", method);
            let (_, errors) = reviewed_app().write("app/models/follow.rb", &model).emit(target);
            assert_eq!(errors.iter().filter(|e| e.contains("conditions resolve to `follower_id`")).count(), 3,
                "association/foreign-key collisions in either order and shared keys must be refused: {errors:?}");
        }
    }
}

#[test]
fn creation_blocks_cannot_run_after_initialization_callbacks() {
    for target in [roundhouse::project::BuildTarget::Ruby, roundhouse::project::BuildTarget::Spinel] {
        let (_, errors) = reviewed_app().emit(target);
        // Variable and literal conditions alike: on a model with an initialization callback, the block would run after it.
        assert_eq!(errors.iter().filter(|e| e.contains("creation block must run inside initialize")).count(), 3,
            "blocks cannot overtake initialization callbacks: {errors:?}");
    }
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn bang_record_conditions_run_natively() {
    let script = format!(
        "Db.configure(\":memory:\")\nSchema.statements.each {{ |sql| Db.exec(sql) }}\nActiveRecord.adapter = SqliteAdapter\n{}",
        include_str!("find_or_create_bang_records/probe.rb"),
    );
    app().run_spinel_with_rbs(&script).assert_passes();
}

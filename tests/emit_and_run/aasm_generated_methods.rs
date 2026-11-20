//! The methods aasm defines as the `aasm do … end` block runs (`state :draft` → `draft?`, `event :publish` →
//! `publish!`, `may_publish?`), called from another class's code on a typed record (maybe's `@account.active?`):
//! read from the app's own observed boot, as `roundhouse check` reads them, and answering as aasm 5.5.1 does.

#[path = "../support/prerequisite.rs"]
mod prerequisite;
use super::aasm_gem::{app, KNOWN_GAPS};

const CALLER: &str = "  belongs_to :article\n\n  def article_state\n    article.draft?.to_s + \"/\" + article.may_publish?.to_s\n  end\n\n  def publish_article\n    article.publish!\n    article.published?\n  end\n";

const SCRIPT: &str = r#"
a = Article.create!(title: "Hello", body: "A body long enough")
c = Comment.create!(article: a, commenter: "me", body: "A comment")
got = [c.article_state, c.publish_article, Comment.find(c.id).article_state]
raise "got #{got.inspect}" unless got == ["true/true", true, "false/false"]
puts "aasm methods passed"
"#;

#[test]
#[ignore = "boots the app under its whole bundle (ROUNDHOUSE_BOOT_BUNDLE_PATH), which CI does not install"]
fn aasm_defined_methods_answer_another_classs_calls() {
    if std::env::var_os(roundhouse::observed_shapes::BOOT_BUNDLE_PATH).is_none() {
        prerequisite::missing("ROUNDHOUSE_BOOT_BUNDLE_PATH holding real-blog's bundle with aasm 5.5.1");
        return;
    }
    let Some(app) = app() else { return };
    let run = app.edit("app/models/comment.rb", "  belongs_to :article\n", CALLER).with_observed_boot().run_ruby(SCRIPT);
    let unknown: Vec<_> = run.errors.iter().filter(|e| !KNOWN_GAPS.iter().any(|gap| e.contains(gap))).collect();
    assert!(unknown.is_empty(), "{unknown:#?}");
    assert!(run.program_succeeded(), "{}\n{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("aasm methods passed"), "{}", run.stdout);
}

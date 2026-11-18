//! A branch on a framework's version (`if ActiveRecord::VERSION::MAJOR >= 3`, aasm's
//! `aasm_create_scope`) is settled by the lockfile's version, as the constant is: the emitted
//! method keeps the branch this Rails takes, and the other one, which may call what this
//! Rails no longer has (`named_scope`), is not carried for a target to compile.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;

const METHODS: &str = "  def self.version_branch
    if ActiveRecord::VERSION::MAJOR >= 3
      :modern
    else
      named_scope :old, conditions: {}
    end
  end

  def self.version_unless
    unless ActiveRecord::VERSION::MAJOR < 3 && ActiveRecord::VERSION::MINOR >= 0
      :kept
    else
      named_scope :older
    end
  end
";

#[test]
fn the_branch_this_rails_rules_out_is_not_emitted() {
    let (emitted, errors) = emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            &format!("  validates :title, presence: true\n\n{METHODS}"),
        )
        .emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    let text = std::fs::read_to_string(emitted.join("app/models/article.rb")).expect("article.rb");
    assert!(!text.contains("named_scope"), "{text}");
    let main = emitted.join("main").display().to_string();
    let output = emit_and_run::ruby()
        .arg("-e")
        .arg(format!(
            "require {main:?}\nMain.configure_default_adapter!\np Article.version_branch\np Article.version_unless"
        ))
        .env("BLOG_DB", ":memory:")
        .output()
        .expect("spawn ruby");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), ":modern\n:kept\n");
}

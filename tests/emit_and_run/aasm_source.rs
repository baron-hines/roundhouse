//! aasm compiled from its installed source: `include AASM` gives the model `aasm` (the gem's `included` hook
//! extends it with `AASM::ClassMethods`), and its `aasm column: … do … end` block runs as the class loads. The
//! record starts in the initial state (the hook's `after_initialize`), fires an event, refuses one its state
//! does not allow, persists the state column and answers the state's scope, as aasm 5.5.1 answers on Rails.

#[path = "../support/prerequisite.rs"]
mod prerequisite;
use super::emit_and_run;

const VERSION: &str = "5.5.1";

fn installed_aasm() -> Option<std::path::PathBuf> {
    let out = std::process::Command::new("ruby")
        .args(["-e", &format!("print Gem::Specification.find_by_name('aasm', '{VERSION}').gem_dir")])
        .output();
    match out {
        Ok(out) if out.status.success() => Some(std::path::PathBuf::from(String::from_utf8_lossy(&out.stdout).into_owned())),
        _ => {
            prerequisite::missing(&format!("ruby with aasm {VERSION} installed"));
            None
        }
    }
}

fn files(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(root, &path, out);
        } else {
            out.push(path.strip_prefix(root).unwrap().to_path_buf());
        }
    }
}

const MODEL: &str = r#"class Article < ApplicationRecord
  include AASM

  aasm column: :status do
    state :draft, initial: true
    state :published

    event :publish do
      transitions from: :draft, to: :published
    end
  end
"#;

const SCRIPT: &str = r#"article = Article.create!(title: "Hello", body: "A body long enough")
got = [article.status, article.draft?, article.may_publish?]
article.publish!
got += [article.status, article.published?, Article.find(article.id).status, article.may_publish?]
begin
  article.publish!
  got << :no_raise
rescue AASM::InvalidTransition => e
  got << e.class.name
end
got << Article.published.count
want = ["draft", true, true, "published", true, "published", false, "AASM::InvalidTransition", 1]
raise "got #{got.inspect}" unless got == want
puts "aasm passed"
"#;

fn app() -> Option<emit_and_run::Overlay> {
    let gem = installed_aasm()?;
    let fixture = roundhouse::fixtures::real_blog();
    let lock = std::fs::read_to_string(fixture.join("Gemfile.lock")).unwrap();
    let lock = lock
        .replacen("\n    rack (", &format!("\n    aasm ({VERSION})\n      concurrent-ruby (~> 1.0)\n    rack ("), 1)
        .replacen("DEPENDENCIES\n", "DEPENDENCIES\n  aasm\n", 1);
    let gemfile = std::fs::read_to_string(fixture.join("Gemfile")).unwrap() + "gem \"aasm\"\n";
    let mut overlay = emit_and_run::real_blog()
        .write("Gemfile", &gemfile)
        .write("Gemfile.lock", &lock)
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .edit("db/schema.rb", "  create_table \"articles\", force: :cascade do |t|\n", "  create_table \"articles\", force: :cascade do |t|\n    t.string \"status\"\n")
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", MODEL);
    let mut list = Vec::new();
    files(&gem, &gem.join("lib"), &mut list);
    for file in list {
        let Ok(text) = std::fs::read_to_string(gem.join(&file)) else { continue };
        overlay = overlay.write(&format!("vendor/bundle/ruby/3.4.0/gems/aasm-{VERSION}/{}", file.display()), &text);
    }
    Some(overlay)
}

/// What the gem's own Ruby does that the strict analysis cannot follow, none of which this flow reaches on the Ruby
/// family (which loads the real gem): Module reflection on a class the analysis cannot prove is one.
const KNOWN_GAPS: [&str; 5] = [
    "const_defined? requires a proven class/module object receiver",
    "method_defined? requires a proven class/module object receiver",
    "instance_method requires a proven class/module object receiver",
    "module_eval requires a proven class/module object receiver",
    "instance_methods requires a proven class/module object receiver",
];

#[test]
fn aasm_fires_events_and_persists_the_state_on_ruby() {
    let Some(app) = app() else { return };
    let run = app.run_ruby(SCRIPT);
    let unknown: Vec<_> = run.errors.iter().filter(|e| !KNOWN_GAPS.iter().any(|gap| e.contains(gap))).collect();
    assert!(unknown.is_empty(), "{unknown:#?}");
    for gap in KNOWN_GAPS {
        assert!(run.errors.iter().any(|e| e.contains(gap)), "{gap} is answered now: drop it from KNOWN_GAPS\n{:#?}", run.errors);
    }
    assert!(run.program_succeeded(), "{}\n{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("aasm passed"), "{}", run.stdout);
}

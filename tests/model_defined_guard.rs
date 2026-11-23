//! A model-body `if` whose branch only loads (`include` / `require`).
//!
//! ```ruby
//! class Post < ApplicationRecord
//!   if defined?(Tracking::Auditable)
//!     include Tracking::Auditable
//!   end
//!   has_many :comments
//! end
//! ```
//!
//! is the "include it when the module is there" idiom (#2282). Since
//! #2275 a branch holding only `include` / `require` counted as a
//! conditional definition, so the whole model was refused as
//! "conditional or dynamic visibility declarations are not modeled".
//! Ruby runs the branch only when the constant is defined: when nothing
//! the app or its gem sources define could be it, the branch is dropped;
//! when the app defines exactly the module it includes, the branch is
//! that include. Any other load-only conditional is refused by name.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::project::{target_files, BuildTarget};

const SCHEMA: &str = r#"ActiveRecord::Schema.define(version: 1) do
  create_table "posts", force: :cascade do |t|
    t.string "title"
  end
  create_table "comments", force: :cascade do |t|
    t.integer "post_id"
    t.string "body"
  end
end
"#;

const AUDITABLE: &str = "module Tracking\n  module Auditable\n    def audited?\n      true\n    end\n  end\nend\n";

const GUARDED: &str = "  if defined?(Tracking::Auditable)\n    include Tracking::Auditable\n  end\n";
const PLAIN: &str = "  include Tracking::Auditable\n";

fn post(head: &str) -> String {
    format!("class Post < ApplicationRecord\n  has_many :comments\n{head}end\n")
}

/// `__rh_value_0_20` as `__rh_value_N_N`: the lowering numbers its
/// temporaries by counters the statements before them move.
fn without_temp_numbers(line: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if word.starts_with("__rh_") {
            let mut last_digit = false;
            for c in word.chars() {
                if c.is_ascii_digit() {
                    if !last_digit {
                        out.push('N');
                    }
                    last_digit = true;
                } else {
                    out.push(c);
                    last_digit = false;
                }
            }
        } else {
            out.push_str(word);
        }
        word.clear();
    };
    for c in line.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

fn tree(head: &str, auditable: bool) -> HashMap<PathBuf, Vec<u8>> {
    let mut tree: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    tree.insert(PathBuf::from("db/schema.rb"), SCHEMA.as_bytes().to_vec());
    tree.insert(
        PathBuf::from("app/models/application_record.rb"),
        b"class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n".to_vec(),
    );
    tree.insert(
        PathBuf::from("app/models/comment.rb"),
        b"class Comment < ApplicationRecord\n  belongs_to :post\nend\n".to_vec(),
    );
    if auditable {
        tree.insert(PathBuf::from("app/models/concerns/tracking/auditable.rb"), AUDITABLE.as_bytes().to_vec());
    }
    tree.insert(PathBuf::from("app/models/post.rb"), post(head).into_bytes());
    tree
}

/// The emitted `post` model files for a model body ending with `head`.
fn emitted_post(target: BuildTarget, head: &str, auditable: bool) -> String {
    let mut app = ingest_app_from_tree(tree(head, auditable)).expect("ingest");
    roundhouse::session::analyze_and_lower(&mut app);
    let files = target_files(&app, Path::new("fixtures/tiny-blog"), target).expect("emit");
    let mut posts: Vec<(String, String)> = files
        .into_iter()
        .filter(|(p, _)| p.ends_with("models/post.rb") || p.ends_with("models/post.rbs"))
        .collect();
    assert!(!posts.is_empty(), "no post model emitted");
    posts.sort();
    // Source-line markers name the model's own lines, which the block
    // above them shifts; the code is what is compared.
    posts
        .into_iter()
        .map(|(p, c)| {
            let code: Vec<String> = c.lines().filter(|l| !l.starts_with("#<SPINEL_SOURCE>")).map(without_temp_numbers).collect();
            format!("# {p}\n{}", code.join("\n"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn refusal(head: &str, auditable: bool) -> String {
    match ingest_app_from_tree(tree(head, auditable)) {
        Ok(_) => panic!("{head}: the model is not refused"),
        Err(err) => err.to_string(),
    }
}

/// Nothing defines the guarded module: Ruby never runs the branch, and
/// the model is what it would be without it.
#[test]
fn a_defined_guard_of_an_undefined_module_is_nothing() {
    for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
        let guarded = emitted_post(target, GUARDED, false);
        assert_eq!(guarded, emitted_post(target, "", false), "{target:?}");
        assert!(!guarded.contains("Auditable"), "{target:?}:\n{guarded}");
    }
    // An empty `else` changes nothing, and a `require` in the branch never runs either.
    let with_else = "  if defined?(Tracking::Auditable)\n    include Tracking::Auditable\n  else\n  end\n";
    let with_require = "  if defined?(Tracking::Auditable)\n    require \"tracking/auditable\"\n    include Tracking::Auditable\n  end\n";
    for head in [with_else, with_require] {
        assert_eq!(emitted_post(BuildTarget::Ruby, head, false), emitted_post(BuildTarget::Ruby, "", false), "{head}");
    }
}

/// The app defines the module the guard asks for and the branch
/// includes: Ruby runs the branch, so the model includes it.
#[test]
fn a_defined_guard_of_the_module_it_includes_is_the_include() {
    for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
        let plain = emitted_post(target, PLAIN, true);
        assert!(plain.contains("audited?") || plain.contains("Auditable"), "{target:?}: the plain include reaches the model:\n{plain}");
        assert_eq!(emitted_post(target, GUARDED, true), plain, "{target:?}");
    }
}

/// A load-only branch this pass does not settle is refused as what it
/// is, not as a visibility declaration.
#[test]
fn other_load_only_conditionals_are_refused_by_name() {
    let env = "  if ENV[\"X\"]\n    require \"json\"\n  end\n";
    let other = "  if defined?(Tracking::Auditable)\n    include Comparable\n  end\n";
    let negated = "  unless defined?(Tracking::Auditable)\n    include Tracking::Auditable\n  end\n";
    for (head, auditable) in [(env, false), (env, true), (other, true), (negated, false)] {
        let error = refusal(head, auditable);
        assert!(error.contains("load-time conditional `include` or `require`"), "{head}: {error}");
        assert!(!error.contains("visibility"), "{head}: {error}");
    }
}

/// A model-body conditional that defines a method stays refused as before.
#[test]
fn a_conditional_definition_in_a_model_stays_refused() {
    let head = "  if ENV[\"X\"]\n    def helper = 1\n  end\n";
    let error = refusal(head, false);
    assert!(error.contains("conditional or dynamic visibility declarations are not modeled"), "{error}");
}

/// The model's own file declares the guarded module after the guard: Ruby
/// reads `defined?` before that declaration runs, which the recorded
/// constants cannot tell, so the model is refused rather than settled.
#[test]
fn a_guarded_module_the_model_file_declares_is_refused() {
    let head = format!("{GUARDED}  module Tracking\n    module Auditable\n    end\n  end\n");
    for auditable in [false, true] {
        let error = refusal(&head, auditable);
        assert!(error.contains("load-time conditional `include` or `require`"), "{error}");
    }
}

/// `Post::Tracking` shadows the top-level `Tracking` inside `Post`, and
/// holds no `Auditable`: Ruby's `defined?(Tracking::Auditable)` is false
/// there although `Tracking::Auditable` exists, so the guard is not settled.
#[test]
fn a_nearer_namespace_shadowing_the_guard_is_refused() {
    let mut tree = tree(GUARDED, true);
    tree.insert(PathBuf::from("app/models/post/tracking.rb"), b"module Post::Tracking\nend\n".to_vec());
    let error = match ingest_app_from_tree(tree) {
        Ok(_) => panic!("the model is not refused"),
        Err(err) => err.to_string(),
    };
    assert!(error.contains("load-time conditional `include` or `require`"), "{error}");
}

/// `ApplicationRecord::Tracking` is found through `Post`'s superclass
/// before the top-level `Tracking`, and holds no `Auditable`: Ruby's
/// `defined?(Tracking::Auditable)` is false in `Post`, so the guard is not
/// settled.
#[test]
fn an_ancestors_namespace_shadowing_the_guard_is_refused() {
    let mut tree = tree(GUARDED, true);
    tree.insert(
        PathBuf::from("app/models/application_record.rb"),
        b"class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\n\n  module Tracking\n  end\nend\n".to_vec(),
    );
    let error = match ingest_app_from_tree(tree) {
        Ok(_) => panic!("the model is not refused"),
        Err(err) => err.to_string(),
    };
    assert!(error.contains("load-time conditional `include` or `require`"), "{error}");
}

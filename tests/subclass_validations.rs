//! A subclass's parent's validations (#1360): under an abstract parent
//! (case 1), and under a concrete one, the STI shape (case 2).
//! activerecord 8.1.4 (oracle `oracle.rb`, which runs these same classes
//! and script): a subclass record runs the parent's `validate`
//! and `validates`, before its own.
//!
//! The subclass's own synthesized `validate` covered only its own
//! declarations, so the parent's checks never ran on it, and nothing
//! reported it.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const TITLED_RECORD: &str = "class TitledRecord < ApplicationRecord\n  self.abstract_class = true\n\n  validate :title_not_reserved\n  validates :title, length: { maximum: 20 }\n\n  def title_not_reserved\n    errors.add(:title, \"is reserved\") if title == \"admin\"\n  end\nend\n";
const ARTICLE: &str = "class Article < TitledRecord\n  has_many :comments, dependent: :destroy\n\n  validates :title, presence: true\n  validates :body, presence: true, length: { minimum: 10 }\n  validate :body_not_placeholder\n\n  def body_not_placeholder\n    errors.add(:body, \"is placeholder\") if body == \"lorem ipsum dolor\"\n  end\nend\n";
const SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
# Rails' errors answer `full_messages`; this runtime's errors are the messages.
messages = ->(record) { record.valid?; record.errors.respond_to?(:full_messages) ? record.errors.full_messages : record.errors }
body = "A body long enough to validate."
check("the parent's validate", messages.(Article.new(title: "admin", body: body)), ["Title is reserved"])
check("the parent's validates", messages.(Article.new(title: "t" * 21, body: body)), ["Title is too long (maximum is 20 characters)"])
check("the subclass's own validate", messages.(Article.new(title: "ok", body: "lorem ipsum dolor")), ["Body is placeholder"])
check("both, the parent's first", messages.(Article.new(title: "admin", body: "lorem ipsum dolor")), ["Title is reserved", "Body is placeholder"])
check("a valid record", Article.new(title: "ok", body: body).valid?, true)
puts "subclass validations passed"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/models/titled_record.rb", TITLED_RECORD)
        .write("app/models/article.rb", ARTICLE)
}

#[test]
fn a_subclass_runs_its_abstract_parents_validations_on_ruby() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("subclass validations passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_subclass_runs_its_abstract_parents_validations_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("subclass validations passed"), "{}", run.stdout);
}

// Case 2 (oracle `sti_script.rb`): a subclass of a concrete model, Rails'
// STI shape, runs its ancestors' validations and then its own
// `validate`, `validates` and `validates_with`; one with none of its
// own runs its parent's. Its declarations were dropped (reported), so only
// the parent's ran.
const SUMMARY_VALIDATOR: &str = "class SummaryValidator < ActiveModel::Validator\n  def validate(record)\n    record.errors.add(:base, \"needs a summary\") unless record.body.to_s.include?(\"summary\")\n  end\nend\n";
const FEATURE: &str = "class Feature < Article\n  validate :title_not_lowercase\n  validates :title, length: { minimum: 3 }\n  validates_with SummaryValidator\n\n  def title_not_lowercase\n    errors.add(:title, \"is lowercase\") if title.to_s == title.to_s.downcase\n  end\nend\n";
const NOTE: &str = "class Note < Article\nend\n";
const STI_SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
messages = ->(record) { record.valid?; record.errors.respond_to?(:full_messages) ? record.errors.full_messages : record.errors }
body = "A body long enough to validate."
check("an STI subclass: the ancestors' first, then its own validate, validates and validates_with",
  messages.(Feature.new(title: "ab", body: "lorem ipsum dolor")),
  ["Body is placeholder", "Title is lowercase", "Title is too short (minimum is 3 characters)", "needs a summary"])
check("the parent's abstract parent too", messages.(Feature.new(title: "admin", body: "A summary that is long enough.")), ["Title is reserved", "Title is lowercase"])
check("a valid subclass record", Feature.new(title: "Hello", body: "A summary that is long enough.").valid?, true)
check("the parent doesn't run the subclass's", messages.(Article.new(title: "ab", body: body)), [])
check("a subclass with no validations of its own", messages.(Note.new(title: "admin", body: body)), ["Title is reserved"])
puts "sti validations passed"
"##;

fn sti_app() -> emit_and_run::Overlay {
    app()
        .write("app/models/summary_validator.rb", SUMMARY_VALIDATOR)
        .write("app/models/feature.rb", FEATURE)
        .write("app/models/note.rb", NOTE)
}

#[test]
fn an_sti_subclass_runs_its_parents_validations_then_its_own_on_ruby() {
    let run = sti_app().run_ruby(STI_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("sti validations passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_sti_subclass_runs_its_parents_validations_then_its_own_on_spinel() {
    let run = sti_app().run_spinel_with_db(STI_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("sti validations passed"), "{}", run.stdout);
}

// Reviews 4381621337 and 4382931974 (oracle `tableless_chain_oracle.rb`,
// which runs these classes and script under activemodel 8.1.4): a
// tableless chain three deep, the leaf running each validation once. The
// middle class is tableless by its parent, and the tableless pass had
// already copied its validations down when the abstract-parent pass
// copied them again. The root's `validates_with` runs on a child and a
// grandchild: the tableless pass didn't copy it.
const CHAIN_VALIDATOR: &str = "class ChainNameValidator < ActiveModel::Validator\n  def validate(record)\n    record.errors.add(:base, \"needs a longer name\") if record.name.to_s.length < 2\n  end\nend\n";
const CHAIN_BASE: &str = "class ChainBaseForm\n  include ActiveModel::Model\n  include ActiveModel::Attributes\n\n  attribute :name, :string\n  validates :name, presence: true\n  validates_with ChainNameValidator\nend\n";
const CHAIN_MIDDLE: &str = "class ChainMiddleForm < ChainBaseForm\n  attribute :code, :string\n  validates :code, presence: true\n  validate :code_not_reserved\n\n  def code_not_reserved\n    errors.add(:code, \"is reserved\") if code.nil? || code == \"admin\"\n  end\nend\n";
const CHAIN_LEAF: &str = "class ChainLeafForm < ChainMiddleForm\nend\n";
const CHAIN_SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
messages = ->(record) { record.valid?; record.errors.respond_to?(:full_messages) ? record.errors.full_messages : record.errors }
check("the root's and the middle class's validations, each once", messages.(ChainLeafForm.new), ["Name can't be blank", "needs a longer name", "Code can't be blank", "Code is reserved"])
check("the middle class's, each once", messages.(ChainMiddleForm.new(name: "nm", code: "admin")), ["Code is reserved"])
check("the root's validates_with, on a child", messages.(ChainMiddleForm.new(name: "n", code: "c")), ["needs a longer name"])
check("and on a grandchild", messages.(ChainLeafForm.new(name: "n", code: "c")), ["needs a longer name"])
check("a valid leaf", ChainLeafForm.new(name: "nm", code: "c").valid?, true)
puts "tableless chain passed"
"##;

fn chain_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/models/chain_name_validator.rb", CHAIN_VALIDATOR)
        .write("app/models/chain_base_form.rb", CHAIN_BASE)
        .write("app/models/chain_middle_form.rb", CHAIN_MIDDLE)
        .write("app/models/chain_leaf_form.rb", CHAIN_LEAF)
}

#[test]
fn a_tableless_chains_leaf_runs_each_validation_once_on_ruby() {
    let run = chain_app().run_ruby(CHAIN_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("tableless chain passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_tableless_chains_leaf_runs_each_validation_once_on_spinel() {
    let run = chain_app().run_spinel_with_db(CHAIN_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("tableless chain passed"), "{}", run.stdout);
}

// Review 4381621327: an ingested App round-trips through JSON
// (`ingested_app_is_self_consistent`), its STI `validates_with` guards
// too. Keyed by a (file, offset) tuple, a non-empty map didn't serialize:
// JSON keys are strings.
#[test]
fn an_apps_sti_validates_with_guards_round_trip() {
    use std::collections::HashMap;
    use std::path::PathBuf;
    let schema = "ActiveRecord::Schema.define(version: 1) do\n  create_table :articles do |t|\n    t.string :title\n    t.text :body\n    t.string :type\n  end\nend\n";
    let mut tree: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    for (path, source) in [
        ("db/schema.rb", schema),
        ("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n"),
        ("app/models/titled_record.rb", TITLED_RECORD),
        ("app/models/article.rb", "class Article < TitledRecord\n  validates :title, presence: true\nend\n"),
        ("app/models/summary_validator.rb", SUMMARY_VALIDATOR),
        ("app/models/feature.rb", FEATURE),
    ] {
        tree.insert(PathBuf::from(path), source.as_bytes().to_vec());
    }
    let app = roundhouse::ingest::ingest_app_from_tree(tree).expect("ingest");
    assert_eq!(app.validates_with_guards.len(), 1, "{:?}", app.validates_with_guards);
    let json = serde_json::to_string(&app).expect("serialize");
    let back: roundhouse::App = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.validates_with_guards, app.validates_with_guards);
}

//! An STI subclass's queries (#1415), with a `type` column: real-blog's
//! articles with one, `Feature < Article`, `SpecialFeature < Feature`,
//! and the calls in app code (`StiProbe`), where `lower::sti_scope`
//! rewrites them. activerecord 8.1.4: a subclass's rows include its own
//! subclasses' (`type IN ('Feature', 'SpecialFeature')`), and its
//! `find`, `sum`, `take`, `find_each`, `find_or_create_by`, `select`,
//! `none`, `pick` and the base's named scopes are scoped like `count`.
//!
//! The classes, the probe and the script are in `tests/sti_queries/`,
//! and its oracle runs them on activerecord 8.1.4:
//!
//!     ruby tests/sti_queries/oracle.rb

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const FEATURE: &str = include_str!("sti_queries/feature.rb");
const SPECIAL_FEATURE: &str = include_str!("sti_queries/special_feature.rb");
const STI_PROBE: &str = include_str!("sti_queries/probe.rb");
const SCRIPT: &str = include_str!("sti_queries/script.rb");
const HOOKED: &str = include_str!("sti_queries/hooked.rb");
const HOOK_PROBE: &str = include_str!("sti_queries/hook_probe.rb");
const HOOK: &str = "  after_initialize :mark_hooked\n\n  def mark_hooked\n    @hook_calls = (@hook_calls || 0) + 1\n    @hook_title = title\n  end\n\n  def hook_seen\n    [@hook_calls, @hook_title]\n  end\n\n";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "create_table \"articles\", force: :cascade do |t|\n    t.string \"title\"\n",
            "create_table \"articles\", force: :cascade do |t|\n    t.string \"title\"\n    t.string \"type\"\n",
        )
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy\n",
            "  has_many :comments, dependent: :destroy\n  scope :titled_a_or_f, -> { where(title: [\"a\", \"f\"]) }\n",
        )
        .write("app/models/feature.rb", FEATURE)
        .write("app/models/special_feature.rb", SPECIAL_FEATURE)
        .write("app/models/sti_probe.rb", STI_PROBE)
}

#[test]
fn an_sti_subclasss_queries_are_scoped_to_it_and_its_subclasses_on_ruby() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("sti queries passed"), "{}", run.stdout);
}

// With `after_initialize` the base hydrates through
// `Article.new(ActiveRecord::Base::HYDRATE_ATTRS)`, and each row is
// still built as its own class. The hook runs once, after the row's
// columns are assigned: it counts its calls and keeps the title it saw.
// The subclass's reads are in app code (`HookProbe`), where
// `lower::sti_scope` scopes them.
#[test]
fn a_hooked_sti_bases_rows_are_built_as_their_class_on_ruby() {
    let run = app()
        .edit("app/models/article.rb", "  validates :title, presence: true\n", &format!("{HOOK}  validates :title, presence: true\n"))
        .write("app/models/hook_probe.rb", HOOK_PROBE)
        .run_ruby(&format!("{SCRIPT}{HOOKED}"));
    run.assert_passes();
    assert!(run.stdout.contains("hooked sti queries passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_sti_subclasss_queries_are_scoped_to_it_and_its_subclasses_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("sti queries passed"), "{}", run.stdout);
}

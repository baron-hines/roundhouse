//! `normalizes` on an enum column (activerecord 8.1.4): the lambda gets
//! the label, however the value was written, runs once per write, and
//! what it answers is what the column stores.

#[path = "support/scratch.rs"]
mod scratch;
#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const MODEL: &str = r#"class Article < ApplicationRecord
  enum :state, { draft: 0, published: 1 }
  normalizes :state, with: ->(v) { Probe.seen << v; v.to_s == "draft" ? "published" : v }
  enum :kind, { x: "ex", y: "why" }
  normalizes :kind, with: ->(v) { Probe.seen << v; v == "x" ? "y" : v }
  enum :other, { p: 0, q: 1 }
  normalizes :other, with: ->(v) { Probe.seen << v; "q" }, apply_to_nil: true
  enum :flag, { unset: nil, on: 1 }
  normalizes :flag, with: ->(v) { Probe.seen << v; v }

  def self.where_draft
    where(state: "draft").count
  end

  def self.where_published
    where(state: "published").count
  end

  def self.where_argument(input)
    where(state: input).count
  end

  def self.where_labels
    where(state: ["draft", "published"]).count
  end

  def self.find_draft
    find_by(state: "draft")
  end

  def self.normalized_state(raw)
    normalize_value_for(:state, raw)
  end

"#;

const SCRIPT: &str = r##"
$bad = []
def check(label, got, want)
  $bad << "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
def state_after(input)
  Probe.seen.clear
  a = Article.new
  a.state = input
  [a.state, Probe.seen.dup]
end
check("label", state_after("draft"), ["published", ["draft"]])
check("symbol", state_after(:draft), ["published", ["draft"]])
check("stored value", state_after(0), ["published", ["draft"]])
check("other label", state_after("published"), ["published", ["published"]])
check("other stored value", state_after(1), ["published", ["published"]])
check("nil", state_after(nil), [nil, []])
check("blank", state_after(""), [nil, []])
Probe.seen.clear
begin
  Article.new.state = "bogus"
  check("bogus raises", false, true)
rescue ArgumentError => e
  check("bogus message", e.message, "'bogus' is not a valid state")
end
check("bogus doesn't reach the lambda", Probe.seen, [])

Probe.seen.clear
check("new", Article.new(state: "draft").state, "published")
check("new calls", Probe.seen, ["draft"])

def kind_after(input)
  Probe.seen.clear
  a = Article.new
  a.kind = input
  [a.kind, Probe.seen.dup]
end
check("string enum, label", kind_after("x"), ["y", ["x"]])
check("string enum, stored value", kind_after("ex"), ["y", ["x"]])
check("string enum, symbol", kind_after(:y), ["y", ["y"]])
check("string enum, other value", kind_after("why"), ["y", ["y"]])

Probe.seen.clear
a = Article.new
a.other = nil
check("apply_to_nil", a.other, "q")
check("apply_to_nil calls", Probe.seen, [nil])

Probe.seen.clear
a = Article.new
a.flag = "on"
check("nil label mapping", a.flag, "on")
check("nil label mapping calls", Probe.seen, ["on"])

Article.delete_all
Probe.seen.clear
a = Article.create!(title: "t", body: "A body long enough to validate.", state: "draft")
check("create", a.state, "published")
check("reload", Article.find(a.id).state, "published")
check("create calls", Probe.seen, ["draft"])

Probe.seen.clear
check("where label", Article.where_draft, 1)
check("where calls", Probe.seen, ["draft"])
check("where other label", Article.where_published, 1)
check("find_by", Article.find_draft&.id, a.id)
check("array", Article.where_labels, 1)
check("where argument", Article.where_argument("draft"), 1)
check("normalize_value_for label", Article.normalized_state("draft"), "published")
check("normalize_value_for stored value", Article.normalized_state(0), "published")
check("normalize_value_for nil", Article.normalized_state(nil), nil)

raise $bad.join("\n") unless $bad.empty?
puts "enum normalizer passed"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "create_table \"articles\", force: :cascade do |t|",
            "create_table \"articles\", force: :cascade do |t|\n    t.integer \"state\"\n    t.string \"kind\"\n    t.integer \"other\"\n    t.integer \"flag\"",
        )
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", MODEL)
        .write("app/models/probe.rb", "class Probe\n  SEEN = []\n\n  def self.seen\n    SEEN\n  end\nend\n")
}

#[test]
fn the_lambda_gets_the_label_once_per_write() {
    app().run_ruby(SCRIPT).assert_passes();
}

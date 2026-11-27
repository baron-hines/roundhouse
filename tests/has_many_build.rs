//! A has_many's `build` and `<<` from app code (#1364). activerecord 8.1.4
//! (oracle `oracle.rb`, which runs real-blog's Article and Comment with
//! these two methods and this script) keeps a record built on, or pushed
//! onto, an owner in the association: the owner's reader answers it before
//! the first save, the owner validates it ("Comments is invalid") and saves
//! it after itself with the key. `<<` on a persisted owner saves the record
//! at once; an invalid one stays unsaved.
//!
//! A new owner's built record had key 0, wasn't in the reader, and was
//! neither validated nor saved; `<<` appended to the Array the reader
//! returned and saved nothing.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const METHODS: &str = "  has_many :comments, dependent: :destroy\n\n  def add_built(commenter, body)\n    comments.build(commenter: commenter, body: body)\n  end\n\n  def add_pushed(comment)\n    comments << comment\n  end\n";

const SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
body = "A body long enough to validate."
a = Article.new(title: "t", body: body)
c = a.add_built("ann", "hi")
# The key before the owner's save is nil in Rails and 0 here (the
# runtime's unsaved id): a stated divergence, not checked.
check("new owner: build, visible in the reader", a.comments.to_a.size, 1)
check("new owner: build then save", [a.save, Comment.where(article_id: a.id).count], [true, 1])
b = Article.new(title: "t", body: body)
b.add_built("", "hi")
# Rails' errors answer `full_messages`; this runtime's errors are the messages.
messages = ->(record) { record.errors.respond_to?(:full_messages) ? record.errors.full_messages : record.errors }
check("new owner: an invalid built child", [b.valid?, messages.(b)], [false, ["Comments is invalid"]])
check("new owner: an invalid built child, save", [b.save, b.persisted?], [false, false])
d = Article.new(title: "t", body: body)
pushed = Comment.new(commenter: "bo", body: "yo")
d.add_pushed(pushed)
check("new owner: << holds it", [pushed.persisted?, d.comments.to_a.size], [false, 1])
check("new owner: << then save", [d.save, pushed.persisted?, pushed.article_id == d.id], [true, true, true])
e = Article.create!(title: "t", body: body)
late = Comment.new(commenter: "cy", body: "hey")
e.add_pushed(late)
check("persisted owner: << saves it", [late.persisted?, late.article_id == e.id], [true, true])
bad = Comment.new(commenter: "", body: "x")
e.add_pushed(bad)
check("persisted owner: << an invalid record stays unsaved", bad.persisted?, false)
f = Article.create!(title: "t", body: body)
built = f.add_built("di", "ok")
check("persisted owner: build, then the owner's save", [built.persisted?, f.save, built.persisted?, built.article_id == f.id], [false, true, true, true])
puts "has_many build passed"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit("app/models/article.rb", "  has_many :comments, dependent: :destroy\n", METHODS)
}

#[test]
fn a_has_many_holds_what_is_built_or_pushed_on_ruby() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("has_many build passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_has_many_holds_what_is_built_or_pushed_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("has_many build passed"), "{}", run.stdout);
}

// Review of #1395 (4374761933, 4374761938; oracle `review_oracle.rb`):
// `<<` with an Array adds each record, nested Arrays flattened, as Rails'
// `concat(records.flatten)` does; and `<<` onto an owner an expression
// answers (`Article.find(id).comments << c`) holds or saves as on a local.
const MORE_METHODS: &str = "  has_many :comments, dependent: :destroy\n\n  def add_pushed(comment)\n    comments << comment\n  end\n\n  def add_pair(x, y)\n    comments << [x, [y]]\n  end\n\n  def add_named(names)\n    comments << names.map { |n| Comment.new(commenter: n, body: \"hi\") }\n  end\n\n  def push_onto(id, comment)\n    Article.find(id).comments << comment\n  end\n";

const MORE_SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
body = "A body long enough to validate."
c = ->(who) { Comment.new(commenter: who, body: "hi") }
a = Article.new(title: "t", body: body)
a.add_pair(c.("ann"), c.("bo"))
check("new owner: an Array, each held", a.comments.to_a.size, 2)
check("new owner: an Array, saved with the owner", [a.save, Comment.where(article_id: a.id).order(:id).pluck(:commenter)], [true, ["ann", "bo"]])
a.add_named(["cy", "di"])
check("persisted owner: an Array of records, each saved", Comment.where(article_id: a.id).order(:id).pluck(:commenter), ["ann", "bo", "cy", "di"])
pushed = c.("ed")
Article.new(title: "t", body: body).push_onto(a.id, pushed)
check("an owner an expression answers: << saves it", [pushed.persisted?, pushed.article_id == a.id], [true, true])
puts "has_many push review passed"
"##;

fn more_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit("app/models/article.rb", "  has_many :comments, dependent: :destroy\n", MORE_METHODS)
}

#[test]
fn an_array_and_an_owner_expression_are_pushed_as_rails_does_on_ruby() {
    let run = more_app().run_ruby(MORE_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("has_many push review passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_array_and_an_owner_expression_are_pushed_as_rails_does_on_spinel() {
    let run = more_app().run_spinel_with_db(MORE_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("has_many push review passed"), "{}", run.stdout);
}

// Review of #1395 (4375848689, oracle `review2_oracle.rb`): a held child
// whose save raises while the owner saves it. Rails rolls the save back
// and keeps both children held, so the owner's next save saves them all.
// The held list was cleared before the saves, so the next save skipped
// the children; and the owner stayed persisted after the rollback, its
// write enlisted only once its after_save hooks had run.
const FAILING_COMMENT: &str = "  validates :commenter, presence: true\n\n  after_save :explode\n\n  def self.armed\n    @armed\n  end\n\n  def self.armed=(name)\n    @armed = name\n  end\n\n  def explode\n    raise \"boom \" + commenter.to_s if Comment.armed == commenter\n  end\n";

const RETRY_SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
a = Article.new(title: "t", body: "A body long enough to validate.")
a.add_built("ann", "hi")
a.add_built("bo", "hi")
Comment.armed = "bo"
raised = begin
  a.save
  nil
rescue => e
  e.message
end
check("a child's save raises through the owner's", raised, "boom bo")
check("the save is rolled back, and the owner is new again", [a.new_record?, Article.count, Comment.count], [true, 0, 0])
Comment.armed = nil
check("the next save saves every held child", [a.save, Comment.where(article_id: a.id).order(:id).pluck(:commenter)], [true, ["ann", "bo"]])
puts "has_many retry passed"
"##;

fn retry_app() -> emit_and_run::Overlay {
    app().edit("app/models/comment.rb", "  validates :commenter, presence: true\n", FAILING_COMMENT)
}

#[test]
fn a_failed_child_save_keeps_the_held_children_on_ruby() {
    let run = retry_app().run_ruby(RETRY_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("has_many retry passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_failed_child_save_keeps_the_held_children_on_spinel() {
    let run = retry_app().run_spinel_with_db(RETRY_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("has_many retry passed"), "{}", run.stdout);
}

// Review of #1395 (4376777753, 4376777762; oracle `review3_oracle.rb`,
// which runs this script): an owner after_save that raises after its held
// child was saved rolls back, leaving the owner new with its changes
// pending and the child held, and the next save saves it; `<<` onto a
// persisted owner answers nil when the record isn't saved, and an Array
// is all or nothing.
const REVIEW3_METHODS: &str = "  has_many :comments, dependent: :destroy\n\n  after_save :explode\n\n  def self.armed\n    @armed\n  end\n\n  def self.armed=(value)\n    @armed = value\n  end\n\n  def explode\n    raise \"boom\" if Article.armed\n  end\n\n  def add_built(commenter, body)\n    comments.build(commenter: commenter, body: body)\n  end\n\n  def add_pushed(comment)\n    comments << comment\n  end\n";

const REVIEW3_SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
body = "A body long enough to validate."
a = Article.new(title: "t", body: body)
a.add_built("ann", "hi")
Article.armed = true
raised = begin
  a.save
  nil
rescue => e
  e.message
end
check("an owner after_save raises after its held child was saved", raised, "boom")
check("rolled back: the owner is new, its changes pending again", [a.new_record?, Article.count, Comment.count, a.changed?, a.changes_to_save.key?("title")], [true, 0, 0, true, true])
check("the child is still held", a.comments.to_a.size, 1)
Article.armed = false
check("the next save saves it", [a.save, Article.count, Comment.pluck(:commenter)], [true, 1, ["ann"]])
o = Article.create!(title: "o", body: body)
check("<< a valid record onto a persisted owner", o.add_pushed(Comment.new(commenter: "bo", body: "x")).nil?, false)
bad = Comment.new(commenter: "", body: "x")
check("<< an invalid one: nil, and it stays unsaved", [o.add_pushed(bad), bad.persisted?], [nil, false])
c1 = Comment.new(commenter: "cy", body: "x")
c2 = Comment.new(commenter: "", body: "x")
c3 = Comment.new(commenter: "di", body: "x")
check("<< [valid, invalid, valid]: nil, and none is saved", [o.add_pushed([c1, c2, c3]), c1.persisted?, c2.persisted?, c3.persisted?], [nil, false, false, false])
check("the owner's rows", Comment.where(article_id: o.id).order(:id).pluck(:commenter), ["bo"])
puts "has_many review3 passed"
"##;

fn review3_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit("app/models/article.rb", "  has_many :comments, dependent: :destroy\n", REVIEW3_METHODS)
}

#[test]
fn a_rolled_back_owner_keeps_its_changes_and_its_held_child_on_ruby() {
    let run = review3_app().run_ruby(REVIEW3_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("has_many review3 passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_rolled_back_owner_keeps_its_changes_and_its_held_child_on_spinel() {
    let run = review3_app().run_spinel_with_db(REVIEW3_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("has_many review3 passed"), "{}", run.stdout);
}

// Review of #1395 (4378322612, oracle `review4_oracle.rb`, which runs this
// script): with no outer transaction, a save's commit callbacks run after
// its after_* hooks. `save` always opens its own transaction, so the
// write is enlisted and its commit callbacks wait for that COMMIT.
const REVIEW4_METHODS: &str = "  has_many :comments, dependent: :destroy\n\n  after_create :log_create\n  after_update :log_update\n  after_save :log_save\n  after_commit :log_commit\n\n  def self.log\n    @log ||= []\n  end\n\n  def log_create\n    Article.log << \"after_create\"\n  end\n\n  def log_update\n    Article.log << \"after_update\"\n  end\n\n  def log_save\n    Article.log << \"after_save\"\n  end\n\n  def log_commit\n    Article.log << \"after_commit\"\n  end\n";

const REVIEW4_SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
body = "A body long enough to validate."
Article.log.clear
Article.create!(title: "t", body: body)
check("create, no outer transaction: the commit callback after every after_* hook", Article.log, ["after_create", "after_save", "after_commit"])
a = Article.first
Article.log.clear
a.update!(title: "u")
check("update, no outer transaction", Article.log, ["after_update", "after_save", "after_commit"])
puts "commit order passed"
"##;

fn review4_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit("app/models/article.rb", "  has_many :comments, dependent: :destroy\n", REVIEW4_METHODS)
}

#[test]
fn commit_callbacks_run_after_the_after_hooks_on_ruby() {
    let run = review4_app().run_ruby(REVIEW4_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("commit order passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn commit_callbacks_run_after_the_after_hooks_on_spinel() {
    let run = review4_app().run_spinel_with_db(REVIEW4_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("commit order passed"), "{}", run.stdout);
}

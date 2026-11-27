//! Record equality on the Ruby family, as activerecord 8.1.4's: two
//! records are `==` (and `eql?`, with the same `hash`) when they're the
//! same class and the same saved row. An unsaved record holds `id` 0 here
//! (roundhouse#1478), so the `!id.nil?` guard never fired and any two new
//! records of a class were equal: `[a, b].uniq` kept one. Its oracle:
//!
//!     ruby tests/record_equality_new/oracle.rb

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
a = Article.new(title: "t")
b = Article.new(title: "t")
check("two new records", [a == b, a.eql?(b), [a, b].uniq.size], [false, false, 2])
check("a new record is itself", a == a, true)
s = Article.create!(title: "s", body: "a body long enough to pass")
l1 = Article.find(s.id)
l2 = Article.find(s.id)
check("two loads of one row", [l1 == l2, [l1, l2].uniq.size, l1 == s], [true, 1, true])
check("a new record and a loaded one", a == l1, false)
n = Article.new(id: s.id, title: "n")
check("a new record built with a row's id, both ways", [l1 == n, n == l1, l1.hash == n.hash, [l1, n].uniq.size], [true, true, true, 1])
s.destroy
check("a destroyed record and a load of its row", [s == l1, [s, l1].uniq.size], [true, 1])
puts "record equality passed"
"##;

#[test]
fn two_new_records_are_not_equal_on_ruby() {
    let run = emit_and_run::real_blog().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("record equality passed"), "{}", run.stdout);
}

/// A string primary key's unsaved id is `""`, not 0 (CodeRabbit on
/// #1479): two new records there aren't equal either, and a load of a
/// saved one still is.
const UUID_SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
a = Post.new(title: "a")
b = Post.new(title: "b")
check("two new uuid records", [a.id, a == b, a.eql?(b), [a, b].uniq.size], ["", false, false, 2])
p = Post.create!(title: "p")
l1 = Post.find(p.id)
l2 = Post.find(p.id)
check("two loads of one uuid row", [l1 == l2, [l1, l2].uniq.size, l1 == p], [true, 1, true])
puts "uuid record equality passed"
"##;

#[test]
fn two_new_uuid_records_are_not_equal_on_ruby() {
    // The fixture's controller names a filter nothing defines, which isn't
    // what this test is about (as in `belongs_to_writer_clears_cache`).
    let run = emit_and_run::tiny_blog_uuid()
        .edit("app/controllers/posts_controller.rb", "  before_action :authenticate_user\n", "")
        .run_ruby(UUID_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("uuid record equality passed"), "{}", run.stdout);
}

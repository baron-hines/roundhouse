//! `for` loops and `"#@ivar"` interpolation, shared by the Ruby lane and
//! the Spinel lane. The answers are CRuby 3.4's.

use super::emit_and_run;

/// A `for` over an Array (with `next`), over a Range literal (the loop
/// variable and a body local stay visible after the loop, nil when it
/// never ran), an exclusive range, Hash pairs into two targets (with
/// `break`), the loop's own value (its collection, a Hash kept a Hash),
/// `break` with a value, and `"#@tag"`.
pub const METHODS: &str = r##"  validates :title, presence: true

  def self.for_sum(xs)
    total = 0
    for x in xs
      next if x == 2
      total += x
    end
    [total, x]
  end

  def self.for_outer
    x = 10
    for x in [1, 2]
    end
    x
  end

  def self.for_range(n)
    out = []
    for i in 1..n
      out << i * 10
      last = i
    end
    [out, i, last]
  end

  def self.for_exclusive
    out = []
    for i in (0...3)
      out << i
    end
    out
  end

  def self.for_pairs(h)
    out = []
    for k, v in h
      break if k == "stop"
      out << "#{k}=#{v}"
    end
    out
  end

  def self.for_value(xs)
    r = for x in xs
      x
    end
    r
  end

  def self.for_hash_value(h)
    r = for k, v in h
    end
    r
  end

  def self.for_break(xs)
    for x in xs
      break x * 10 if x > 1
    end
  end

  def tagged
    @tag = "T"
    "#@tag:#{title}"
  end
"##;

pub const ASSERTIONS: &str = r##"raise "sum #{Article.for_sum([1, 2, 3]).inspect}" unless Article.for_sum([1, 2, 3]) == [4, 3]
raise "empty #{Article.for_sum([]).inspect}" unless Article.for_sum([]) == [0, nil]
raise "outer #{Article.for_outer.inspect}" unless Article.for_outer == 2
raise "range #{Article.for_range(3).inspect}" unless Article.for_range(3) == [[10, 20, 30], 3, 3]
raise "range0 #{Article.for_range(0).inspect}" unless Article.for_range(0) == [[], nil, nil]
raise "excl #{Article.for_exclusive.inspect}" unless Article.for_exclusive == [0, 1, 2]
raise "pairs #{Article.for_pairs({ "a" => 1, "b" => 2, "stop" => 3, "c" => 4 }).inspect}" unless Article.for_pairs({ "a" => 1, "b" => 2, "stop" => 3, "c" => 4 }) == ["a=1", "b=2"]
raise "value #{Article.for_value([5, 6]).inspect}" unless Article.for_value([5, 6]) == [5, 6]
raise "hash value" unless Article.for_hash_value({ "a" => 1 }) == { "a" => 1 }
raise "break #{Article.for_break([1, 2, 3]).inspect}" unless Article.for_break([1, 2, 3]) == 20
raise "tag" unless Article.new(title: "x", body: "a body that is long enough").tagged == "T:x"
puts "for loops passed"
"##;

#[allow(dead_code)]
pub fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit("app/models/article.rb", "  validates :title, presence: true\n", METHODS)
}

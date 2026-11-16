//! Synthetic repros shared by Ruby and native declaration-composition checks.
use super::emit_and_run;

pub const CASES: &[(&str, &str, &str, &str)] = &[
    ("qualified constants", r##"Point = Data.define(:x, :y)
Point::ORIGIN = Point.new(x: 0, y: 0)
Point::COPY = Point::ORIGIN
"##, r##"p Point::ORIGIN
p Point::COPY == Point.new(0, 0)
p Point::ORIGIN.with(x: 3).to_h
"##, "#<data Point x=0, y=0>\ntrue\n{x: 3, y: 0}\n"),
    ("constant and method reopening", r##"Point = Data.define(:x, :y)
class Point
  ORIGIN = new(x: 0, y: 0)
  COPY = ORIGIN
  def norm = x.abs + y.abs
  def self.origin = ORIGIN
end
"##, r##"p Point.origin.norm
p Point::COPY == Point.origin
p Point.origin.with(x: -3).norm
p Point.origin.to_h
p Point.origin
"##, "0\ntrue\n3\n{x: 0, y: 0}\n#<data Point x=0, y=0>\n"),
    ("reopening around first use", r##"Point = Data.define(:x) do
  def label = "before"
end
class Point
  def label = "after"
  FIRST = new(x: 1)
  LAST = new(x: 2)
end
"##, "puts Point::FIRST.label\nputs Point::LAST.label\nputs Point.new(3).label\n",
        "after\nafter\nafter\n"),
    ("private_class_method constructors", r##"Token = Data.define(:value) do
  private_class_method :new, :[], :allocate
  def self.issue(v) = new(value: v)
end
"##, PRIVATE_ASSERTIONS, PRIVATE_EXPECTED),
    ("singleton private constructors", r##"Token = Data.define(:value) do
  class << self
    private :new, :[], :allocate
  end
  def self.issue(v) = new(value: v)
end
"##, PRIVATE_ASSERTIONS, PRIVATE_EXPECTED),
    ("nested delegate and method", r##"class Entry
  def code = "abc"
end
class Envelope
  View = Data.define(:entry) do
    delegate :code, to: :entry
    def label = code
  end
end
"##, r##"entry = Entry.new
view = Envelope::View.new(entry: entry)
puts view.code
puts view.label
p view.to_h == {entry: entry}
p view == view.with
puts view.inspect.start_with?("#<data Envelope::View entry=")
"##, "abc\nabc\ntrue\ntrue\ntrue\n"),
    ("prefixed nil delegate and method", r##"class Entry
  def code = "abc"
end
class Envelope
  View = Data.define(:entry) do
    delegate :code, to: :entry, prefix: true, allow_nil: true
    def label = entry_code
  end
end
"##, r##"entry = Entry.new
view = Envelope::View.new(entry: entry)
puts view.entry_code
puts view.label
p view.with(entry: nil).entry_code
p view.to_h == {entry: entry}
p view == view.with
puts view.inspect.start_with?("#<data Envelope::View entry=")
"##, "abc\nabc\nnil\ntrue\ntrue\ntrue\n"),
    ("changing return around first use", r##"Point = Data.define(:x) do
  def label = 1
end
class Point
  def label = "after"
  FIRST = new(1)
  LAST = new(2)
end
"##, "p Point::FIRST.label\np Point::LAST.label\n", "\"after\"\n\"after\"\n"),
    ("delegate override", r##"class Entry
  def code = "abc"
end
class Envelope
  View = Data.define(:entry) do
    delegate :code, to: :entry
    def label = code
    def code = "override"
  end
end
"##, "puts Envelope::View.new(Entry.new).label\n", "override\n"),
    ("nil delegation errors", r##"class Entry
  def code = "entry"
end
class Envelope
  NoMethodError = RuntimeError
  View = Data.define(:entry) do
    delegate :code, to: :entry, prefix: true
    delegate :nil?, to: :entry, prefix: true
    def label = entry_code
  end
end
"##, r##"begin
  Envelope::View.new(nil).label
rescue ActiveSupport::DelegationError => error
  puts error.message.lines.first
  p error.is_a?(NoMethodError)
end
p Envelope::View.new(nil).entry_nil?
puts Envelope::View.new(Entry.new).label
"##, "entry_code delegated to entry, but entry is nil\ntrue\ntrue\nentry\n"),
    ("literal initializer forms", r##"Scalar = Data.define(:value)
Scalar::LITERAL = {values: [1, -2, 3.5], text: "plain", flag: true, absent: nil, symbol: :ok, off: false}
Scalar::ROOT = Scalar.new(Scalar::LITERAL)
Point = Data.define(:value)
class Point
  LITERAL = [1, 2]
  ORIGIN = new(value: LITERAL)
  BRACKET = Point.[](LITERAL)
  NAMED = Point.new(LITERAL)
  COPY = ORIGIN
  PREVIOUS = Scalar::ROOT
end
"##, r##"p Point::ORIGIN
p Point::BRACKET
p Point::NAMED
p Point::COPY
p Point::PREVIOUS.value[:flag]
p Scalar::ROOT.value[:text]
p Point::ORIGIN.with(value: [3]).to_h
"##, "#<data Point value=[1, 2]>\n#<data Point value=[1, 2]>\n#<data Point value=[1, 2]>\n#<data Point value=[1, 2]>\ntrue\n\"plain\"\n{value: [3]}\n"),
    ("base visibility forms with a mixin", r###"module RecordHelpers
  def stamp = 'helper'
end
Record = Data.define(:value) do
  include RecordHelpers
  private
  def secret = value
  protected
  def hidden = value
  public
  def label = secret
  private def inline_secret = value
  def named = value
  private 'named'
  def self.build(v) = new(v)
  private_class_method 'build'
  def self.issue(v) = build(v)
  public_class_method 'issue'
end
"###, r###"record = Record.issue('ok')
puts record.label
p record.respond_to?(:secret)
p record.respond_to?(:hidden)
p record.respond_to?(:inline_secret)
p record.respond_to?(:named)
p record.to_h
"###, "ok\nfalse\nfalse\nfalse\nfalse\n{value: \"ok\"}\n"),

];
pub const PRIVATE_ASSERTIONS: &str = r##"token = Token.issue(7)
p token
p token.with(value: 8).to_h
p token == Token.issue(7)
begin
  Token.new(value: 8)
  raise "new became public"
rescue NoMethodError
  puts "new: private"
end
begin
  Token[8]
  raise "[] became public"
rescue NoMethodError
  puts "[]: private"
end
begin
  Token.allocate
  raise "allocate became public"
rescue NoMethodError
  puts "allocate: private"
end
"##;
pub const PRIVATE_EXPECTED: &str = "#<data Token value=7>\n{value: 8}\ntrue\nnew: private\n[]: private\nallocate: private\n";

pub fn app(source: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write("db/schema.rb", "")
        .write("config/database.yml", "test:\n  primary:\n    adapter: sqlite3\n    database: storage/test.sqlite3\n")
        .write("lib/example.rb", source)
}

// Reviewed declaration boundaries; each was red on 25140c67e.
pub const REVIEW_CASES: &[(&str, &str, &str, &str, &str)] = &[
    (r####"qualified_overwrite"####, r####"Point = Data.define(:x)
Point::VALUE = 1
class Observer
  VALUE = Point::VALUE
end
Point::VALUE = 2
"####, r####"p [Point::VALUE, Observer::VALUE]
"####, r####"refuse"####, r####"Data.define declaration group"####),
    (r####"qualified_cycle"####, r####"Point = Data.define(:x)
Point::FIRST = 1
class Observer
  VALUE = Point::FIRST + 1
end
Point::LAST = Observer::VALUE + 1
"####, r####"p [Point::FIRST, Observer::VALUE, Point::LAST]
"####, r####"refuse"####, r####"Data.define declaration group"####),
    (r####"qualified_reopening_scope"####, r####"module Outside
  String = Integer
end
Outside::Point = Data.define(:x)
class Outside::Point
  TYPE = String.name
end
"####, r####"p Outside::Point::TYPE
"####, r####"refuse"####, r####"qualified Data reopening"####),
    (r####"private_subclass_alias"####, r####"Token = Data.define(:value) do
  private_class_method :new, :[], :allocate
  def self.issue(v) = new(value: v)
end
Parent = Token
class Child < Parent
end
"####, r####"p Child.issue(7).class.name
"####, r####"refuse"####, r####"constructor visibility with Data subclasses"####),
    (r####"delegate_macro_override"####, r####"class Entry
  def code = 'entry'
end
class Envelope
  View = Data.define(:entry) do
def code = 'own'
def self.delegate(name, to:)
  nil
end
delegate :code, to: :entry
def label = code

  end
end
"####, r####"begin; puts Envelope::View.new(Entry.new).label; rescue NoMethodError; puts 'missing'; end
"####, r####"refuse"####, r####"app-defined delegate"####),
    (r####"delegate_constant_target"####, r####"class Provider
  def self.code = 'outer'
end
class Envelope
  class Provider
    def self.code = 'inner'
  end
  View = Data.define(:entry) do
    delegate :code, to: :Provider
    def label = code
  end
end
"####, r####"puts Envelope::View.new(1).label
"####, r####"refuse"####, r####"Data delegate constant target"####),
    (r####"delegate_nil_denied"####, r####"class Entry
  def code = 'entry'
end
class Envelope
  View = Data.define(:entry) do
delegate :code, to: :entry
    def label = code
  end
end
"####, r####"view = Envelope::View.new(nil)
begin; p view.label; rescue => e; puts e.class.name; end
"####, r####"match"####, r####"ActiveSupport::DelegationError
"####),
    (r####"delegate_positional_hash"####, r####"class Entry
  def code = 'entry'
end
class Envelope
  View = Data.define(:entry) do
delegate :code, {to: :entry}
    def label = code
  end
end
"####, r####"view = Envelope::View.new(Entry.new)
puts view.label
begin; puts view.code; rescue NoMethodError; puts 'private'; end
"####, r####"refuse"####, r####"Data delegate requires keyword options"####),
    (r####"constant_member_override"####, r####"Point = Data.define(:x)
class Point
  FIRST = new(3).x
  def x = 'four'
  LAST = new(3).x
end
"####, r####"p [Point::FIRST, Point::LAST]
"####, r####"refuse"####, r####"Data constant initializer is not a literal or a constructor call"####),
    (r####"inherited_data_method_after_use"####, r####"Point = Data.define(:x)
class Point
  FIRST = new(1).to_h
  def to_h = 'override'
  LAST = new(2).to_h
end
"####, r####"p [Point::FIRST, Point::LAST]
"####, r####"refuse"####, r####"Data constant initializer is not a literal or a constructor call"####),
    (r####"qualified_expression_bytes"####, r####"Point = Data.define(:x)
Point::VALUE = 3.bytes
"####, r####"begin
  p ::VALUE
rescue NameError
  puts 'root absent'
end
p Point::VALUE
"####, r####"refuse"####, r####"Data constant initializer is not a literal or a constructor call"####),
    (r####"qualified_expression_inquiry"####, r####"Point = Data.define(:x)
Point::VALUE = "ready".inquiry
"####, r####"begin
  p ::VALUE
rescue NameError
  puts 'root absent'
end
p Point::VALUE
"####, r####"refuse"####, r####"Data constant initializer is not a literal or a constructor call"####),
    (r####"qualified_dependency_cycle"####, r####"R = Data.define(:x)
class Holder
  TYPE = R
end
R::ALIAS = Holder::TYPE
"####, r####"p R::ALIAS == R
"####, r####"refuse"####, r####"Data.define declaration group"####),
];

// Narrowing-only recheck cases; all were incorrectly admitted on 2cd94e503.
pub const NARROW_CASES: &[(&str, &str, &str, &str, &str)] = &[
    (r####"group_private_alias_return"####, r####"Token = Data.define(:value) do
  private_class_method :new, :[], :allocate
  def self.issue(v) = new(value: v)
end
class Holder
  def self.type = Token
end
Parent = Holder.type
class Child < Parent
end
"####, r####"p Child.issue(7).class.name
"####, r####"refuse"####, r####"constructor visibility with Data subclasses"####),
    (r####"inherited_instance_dup"####, r####"Point = Data.define(:x)
class Point
  FIRST = new(1).dup
  def dup = with(x: 9)
  LAST = new(2).dup
end
"####, r####"p [Point::FIRST.x, Point::LAST.x]
"####, r####"refuse"####, r####"Data constant initializer is not a literal or a constructor call"####),
    (r####"inherited_instance_clone"####, r####"Point = Data.define(:x)
class Point
  FIRST = new(1).clone
  def clone = with(x: 9)
  LAST = new(2).clone
end
"####, r####"p [Point::FIRST.x, Point::LAST.x]
"####, r####"refuse"####, r####"Data constant initializer is not a literal or a constructor call"####),
    (r####"app_private_constructor_macro"####, r####"Token = Data.define(:value) do
  def self.private_class_method(*names)
    nil
  end
  private_class_method :new, :[], :allocate
  def self.issue(v) = new(value: v)
end
"####, r####"p Token.new(7).value
p Token.issue(8).value
"####, r####"refuse"####, r####"Data visibility macro"####),
    (r####"inherited_class_members"####, r####"Point = Data.define(:x)
class Point
  FIRST = members
  def self.members = [:other]
  LAST = members
end
"####, r####"p [Point::FIRST, Point::LAST]
"####, r####"refuse"####, r####"Data constant initializer is not a literal or a constructor call"####),
    (r####"members_without_override"####, r####"Point = Data.define(:x)
class Point
  FIRST = members
end
"####, r####"p Point::FIRST
"####, r####"refuse"####, r####"Data constant initializer is not a literal or a constructor call"####),
];

pub const LITERAL_CASES: &[(&str, &str, &str, &str, &str)] = &[
    (r####"narrow_external_copy"####, r####"class Copier
  def self.copy(value) = value.dup
end
Point = Data.define(:x)
class Point
  FIRST = Copier.copy(new(1))
  def dup = with(x: 9)
  LAST = Copier.copy(new(2))
end
"####, r####"p [Point::FIRST.x, Point::LAST.x]
"####, r####"refuse"####, r####"Data constant initializer is not a literal or a constructor call"####),
    (r####"narrow_interpolated_to_s"####, r####"Point = Data.define(:x)
class Point
  FIRST = "#{new(1)}"
  def to_s = 'changed'
  LAST = "#{new(2)}"
end
"####, r####"p [Point::FIRST, Point::LAST]
"####, r####"refuse"####, r####"Data constant initializer is not a literal or a constructor call"####),
    (r####"narrow_case_equality"####, r####"Point = Data.define(:x)
class Point
  FIRST = case new(1); when new(1); 'match'; else; 'miss'; end
  def ===(other) = false
  LAST = case new(1); when new(1); 'match'; else; 'miss'; end
end
"####, r####"p [Point::FIRST, Point::LAST]
"####, r####"refuse"####, r####"Data constant initializer is not a literal or a constructor call"####),
    (r####"unrelated_nil_query"####, r####"class Entry
 def code = "abc"
end
class Unrelated
 def nil? = false
end
class Envelope
 View = Data.define(:entry) do
  def entry = Entry.new
  delegate :code, to: :entry
  def label = code
 end
end
"####, r####"puts Envelope::View.new(nil).label
"####, r####"match"####, r####"abc
"####),
    (r####"unrelated_raising_method"####, r####"class Entry
 def code = "abc"
end
class Unrelated
 def code; raise NoMethodError, "unrelated"; end
end
class Envelope
 View = Data.define(:entry) do
  def entry = Entry.new
  delegate :code, to: :entry
  def label = code
 end
end
"####, r####"puts Envelope::View.new(nil).label
"####, r####"match"####, r####"abc
"####),
];

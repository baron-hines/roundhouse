//! A class macro that takes a block, shared by the Ruby lane and the
//! Spinel lane: parslet's `rule(:name) { … }` shape. The block runs where
//! the defined body `instance_eval`s it, a `method_defined?` guard is
//! answered from the class's methods, the trailing Hash binds to
//! `opts = {}`, and a value from an included module's constant is the
//! class's (`precedence BASE`). The answers are CRuby 3.4's.

use super::emit_and_run;

pub const RULE_BASE: &str = r##"module Grammar
  LOW = 1
end

class RuleBase
  include Grammar

  def self.rule(name, opts = {}, &definition)
    undef_method name if method_defined? name
    define_method(name) do
      @rules ||= {}
      return @rules[name] if @rules.has_key?(name)
      body = proc { self.instance_eval(&definition) }
      @rules[name] = "#{opts[:label]}#{body.call}"
    end
  end

  # `level LOW` sets the level; every instance reads it through `level`.
  def self.level(value)
    define_method(:level) { value }
  end

  def describe
    "L#{level}"
  end

  def greeting
    "hi"
  end
end
"##;

pub const PHRASES: &str = r##"class Phrases < RuleBase
  level LOW
  rule(:word) { greeting + "!" }
  rule(:twice, label: "x2:") { word * 2 }
end
"##;

pub const ASSERTIONS: &str = r##"p = Phrases.new
raise "word #{p.word.inspect}" unless p.word == "hi!"
raise "twice #{p.twice.inspect}" unless p.twice == "x2:hi!hi!"
raise "memo" unless p.word.equal?(p.word)
raise "level #{p.level.inspect}" unless p.level == 1
raise "describe #{p.describe.inspect}" unless p.describe == "L1"
puts "block macros passed"
"##;

#[allow(dead_code)]
pub fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/rule_base.rb", RULE_BASE).write("app/models/phrases.rb", PHRASES)
}

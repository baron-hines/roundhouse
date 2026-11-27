//! String literals a variable holds, in a file without
//! `# frozen_string_literal: true`, shared by the Ruby lane and the
//! Spinel lane: Ruby gives each evaluation a new mutable String there, so
//! appending to one works, while spinel freezes every bare literal.

use super::emit_and_run;

pub const BUILDER: &str = r##"class Builder
  def self.build
    s = ""
    s << "a"
    t = nil
    t ||= ""
    t << "b"
    [s, t, new.tagged]
  end

  def tagged
    @tag = "x"
    @tag << "y"
    @tag
  end
end
"##;

/// The same literal under the magic comment stays a frozen literal.
pub const FROZEN: &str = r##"# frozen_string_literal: true

class Frozen
  def self.label
    s = "kept"
    s
  end
end
"##;

pub const ASSERTIONS: &str = r##"got = Builder.build
raise "build #{got.inspect}" unless got == ["a", "b", "xy"]
raise "frozen" unless Frozen.label == "kept"
puts "mutable literals passed"
"##;

#[allow(dead_code)]
pub fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/builder.rb", BUILDER).write("app/models/frozen.rb", FROZEN)
}

//! `@line_ends = []` then `@line_ends.extend RangeSearch` (parslet's
//! LineCache, reached by lobsters' `/search` on every parse) used to be
//! the `Object#extend` raise. A fresh empty Array or Hash extended right
//! after with one of the program's modules becomes an instance of a
//! subclass that includes the module, so it keeps both the module's
//! methods and the builtin's.

use super::emit_and_run;

const CACHE: &str = r##"module Scanner
  module RangeSearch
    def lower_bound(value)
      index { |item| item >= value }
    end
  end

  module Labels
    def mark(key, value)
      self[key] = value
    end

    def label(key)
      "#{key}=#{self[key]}"
    end
  end

  class LineCache
    def initialize
      @line_ends = []
      @line_ends.extend RangeSearch
      tags = {}
      tags.extend Labels
      @tags = tags
    end

    def scan(text)
      text.to_s.each_char.with_index { |c, i| @line_ends << i if c == "\n" }
      @tags.mark(:lines, @line_ends.size)
      self
    end

    def line_of(pos)
      @line_ends.lower_bound(pos) || @line_ends.size
    end

    def summary
      @tags.label(:lines)
    end
  end
end
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/scanner.rb", CACHE)
}

const ASSERTIONS: &str = r##"cache = Scanner::LineCache.new.scan("ab\ncd\nef")
got = [cache.line_of(0), cache.line_of(3), cache.line_of(7), cache.summary]
raise got.inspect unless got == [0, 1, 2, "lines=2"]
puts "fresh builtin extend passed"
"##;

#[test]
fn fresh_builtin_extend_runs() {
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("fresh builtin extend passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn fresh_builtin_extend_runs_on_spinel() {
    let run = app().run_spinel(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("fresh builtin extend passed"), "{}", run.stdout);
}

#[test]
fn a_fresh_builtin_extend_is_unsupported_on_strict_targets() {
    use roundhouse::project::BuildTarget::*;
    for target in [Rust, Typescript] {
        let (_emitted, errors) = app().emit(target);
        assert!(errors.iter().any(|e| e.contains("subclasses Array")), "{target:?}: {errors:#?}");
    }
}

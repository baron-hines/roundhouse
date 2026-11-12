//! A gem compiled from source that loads an optional feature's library
//! inside a method body (`require "fancy_printer"` in
//! `Shelf::Pretty.load_dependencies!`), rescuing `LoadError`, as gems
//! with optional dependencies do. The library is in neither the
//! lockfile nor Ruby's standard library. Ruby runs the `require` only
//! when the method is called, and raises `LoadError` there: the gem's
//! rescue answers nil. The answers are Ruby 4.0.7's, on the gem as
//! written; the Spinel tree carries the compiled gem.

use super::emit_and_run;

const GEM: &str = "vendor/bundle/ruby/3.4.0/gems/shelf-1.0.0/lib/shelf.rb";

pub const SHELF: &str = r#"module Shelf
  # An optional feature: its library loads only when a class turns it on.
  module Pretty
    def self.load_dependencies!
      require "fancy_printer"
      true
    rescue LoadError
      nil
    end
  end

  class Box
    def self.pretty!
      Pretty.load_dependencies!
    end

    def self.kind
      "box"
    end
  end
end
"#;

pub const ARTICLE_BOX: &str = "class ArticleBox < Shelf::Box\n  def self.pretty_kind\n    [pretty!.inspect, kind].join(\" \")\n  end\nend\n";

pub const ASSERTIONS: &str = r#"got = ArticleBox.pretty_kind
raise "pretty_kind #{got.inspect}" unless got == "nil box"
puts "gem method require passed"
"#;

#[allow(dead_code)]
pub fn app() -> emit_and_run::Overlay {
    let fixture = roundhouse::fixtures::real_blog();
    let lock = std::fs::read_to_string(fixture.join("Gemfile.lock")).unwrap();
    assert!(lock.contains("\n    rack (") && lock.contains("DEPENDENCIES\n"), "the fixture's Gemfile.lock changed shape");
    let lock = lock.replacen("\n    rack (", "\n    shelf (1.0.0)\n    rack (", 1).replacen("DEPENDENCIES\n", "DEPENDENCIES\n  shelf\n", 1);
    let gemfile = std::fs::read_to_string(fixture.join("Gemfile")).unwrap() + "gem \"shelf\"\n";
    emit_and_run::real_blog()
        .write("Gemfile", &gemfile)
        .write("Gemfile.lock", &lock)
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write(GEM, SHELF)
        .write("app/models/article_box.rb", ARTICLE_BOX)
}

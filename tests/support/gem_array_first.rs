//! A plain-Ruby gem whose class indexes itself: `[]` answers an Array,
//! and a method reads `self[:dn].first`. `first` there is the Array's,
//! not ActiveSupport's `String#first`. Compiled from source on Spinel;
//! the answers are Ruby 4.0.7's.

use super::emit_and_run;

pub const GEM_DIR: &str = "vendor/bundle/ruby/3.4.0/gems/shelf-1.0.0";

pub const SHELF: &str = r##"module Shelf
  class Entry
    def initialize
      @h = {}
    end

    def []=(k, v)
      @h[k] = [v]
    end

    def [](k)
      @h[k] || []
    end

    def dn
      self[:dn].first.to_s
    end
  end
end
"##;

pub const PROBE: &str = r##"class EntryProbe
  def self.describe
    entry = Shelf::Entry.new
    entry[:dn] = "cn=shelf"
    [entry.dn, Shelf::Entry.new.dn]
  end
end
"##;

/// Ruby 4.0.7 answers `["cn=shelf", ""]`.
pub const ASSERTIONS: &str = r##"got = EntryProbe.describe
raise "dn #{got.inspect}" unless got == ["cn=shelf", ""]
puts "entry passed"
"##;

/// real-blog with the `shelf` gem installed under `vendor/bundle`, locked
/// and listed in `source_gems`, and a model naming `Shelf::Entry`.
#[allow(dead_code)]
pub fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    debug (1.11.1)\n    shelf (1.0.0)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  shelf\n")
        .edit("Gemfile", "gem \"rails\", \"~> 8.1.4\"\n", "gem \"rails\", \"~> 8.1.4\"\ngem \"shelf\"\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write(&format!("{GEM_DIR}/lib/shelf.rb"), SHELF)
        .write("app/models/entry_probe.rb", PROBE)
        .write("roundhouse.yml", "extensions:\n  - config/roundhouse\n")
        .write("config/roundhouse/gems.yml", "source_gems:\n  - {gem: \"shelf\"}\n")
}

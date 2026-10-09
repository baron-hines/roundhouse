//! One contract per construct campfire's SQLite-observer caches
//! (rubys/roundhouse#698) brought into the subset, shared by the
//! interpreted (CRuby overlay, `tests/emit_and_run/campfire_caches.rs`)
//! and native (spinel, `tests/spinel_toolchain/campfire_caches.rs`)
//! lanes. Each removed a strict-emit error or an ingest gap; the two
//! lanes run the same source and print the same lines.

use super::emit_and_run::{real_blog, Overlay};

/// A probe the overlay writes, the script that drives it, and the
/// lines the script prints (CRuby's output for the same code).
pub struct Contract {
    pub path: &'static str,
    pub source: &'static str,
    pub script: &'static str,
    pub expected: &'static str,
}

impl Contract {
    pub fn overlay(&self) -> Overlay {
        real_blog().write(self.path, self.source)
    }
}

/// `_, (_, removed_size) = @entries.shift` — `ResponseCache#write`'s
/// eviction. Each group destructures its own element, a scalar group
/// value pads with nil, and the assignment evaluates to its RHS.
pub const NESTED_MULTI_WRITE: Contract = Contract {
    path: "app/models/nested_write_probe.rb",
    source: r#"class NestedWriteProbe
  def initialize
    @entries = { "first" => ["a", 3], "second" => ["b", 5] }
    @bytes = 8
  end

  def evict
    _, (_, removed_size) = @entries.shift
    @bytes -= removed_size
    @bytes
  end

  def remaining
    @entries.length
  end

  def deep
    a, (b, (c, d)) = 1, [2, [3, 4]]
    [a, b, c, d]
  end

  def padded
    a, (b, c) = 1, 2
    [a, b, c.nil?]
  end
end
"#,
    script: r#"probe = NestedWriteProbe.new
puts probe.evict
puts probe.evict
puts probe.remaining
puts probe.deep.inspect
puts probe.padded.inspect
"#,
    expected: "5\n0\n0\n[1, 2, 3, 4]\n[1, 2, true]\n",
};

/// `ContentKey = Data.define(:digest) do def cache_key … end` —
/// `FragmentCache`'s content key. The block's methods (instance and
/// class side) are the Data class's, beside its member readers.
pub const DATA_BLOCK_METHODS: Contract = Contract {
    path: "app/models/data_block_probe.rb",
    source: r#"class DataBlockProbe
  ContentKey = Data.define(:digest) do
    def cache_key
      "key-" + digest
    end

    def self.build(value)
      new(digest: value)
    end
  end

  def self.key(value)
    ContentKey.new(digest: value)
  end
end
"#,
    script: r#"key = DataBlockProbe.key("abc")
puts key.cache_key
puts key.digest
puts DataBlockProbe::ContentKey.build("def").cache_key
puts key == DataBlockProbe::ContentKey.new(digest: "abc")
puts key.is_a?(DataBlockProbe::ContentKey)
"#,
    expected: "key-abc\nabc\nkey-def\ntrue\ntrue\n",
};

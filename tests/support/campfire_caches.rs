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

/// `CachedResponses`' request surface: rack's encoding negotiation
/// (`Rack::Utils`), the gzip body and its weak ETag (`Zlib.gzip`,
/// `String#byteslice`), the session snapshot (`Hash#deep_dup`) and the
/// cache key (`ActiveSupport::JSON.encode`, with `decode` beside it as
/// `RecordCache` reads it back).
pub const RESPONSE_HELPERS: Contract = Contract {
    path: "app/models/response_helpers_probe.rb",
    source: r##"require "zlib"

class ResponseHelpersProbe
  def self.encoding(header)
    Rack::Utils.select_best_encoding(%w[ gzip identity ], Rack::Utils.q_values(header))
  end

  def self.etag
    body = "<p>cached</p>"
    %(W/"#{Digest::SHA256.hexdigest(body).byteslice(0, 32)}")
  end

  def self.past_the_end
    "abc".byteslice(5, 1).nil?
  end

  def self.gzip_round_trip
    body = "<p>cached</p>" * 20
    zipped = Zlib.gzip(body)
    [zipped.byteslice(0, 2).bytes, zipped.bytesize < body.bytesize, Zlib.gunzip(zipped) == body]
  end

  # Equal, but no level shares an object with the original — what the
  # copy is for. (Read, not mutated: mutating through the untyped copy
  # retypes unrelated String-keyed hashes on spinel; see the PR.)
  def self.snapshot
    original = { "user" => { "id" => "1" }, "flash" => [ "a" ] }
    copy = original.deep_dup
    [ copy == original, copy.equal?(original), copy["user"].equal?(original["user"]),
      copy["flash"].equal?(original["flash"]), copy["user"]["id"].equal?(original["user"]["id"]) ]
  end

  def self.key
    ActiveSupport::JSON.encode([ "rooms", "/rooms/1?a=<b>&c", nil, 7, 1.5, true, :html, { "token" => "t", "n" => [ 1, nil ] } ])
  end

  def self.decoded
    ActiveSupport::JSON.decode(key)
  end
end
"##,
    script: r#"puts ResponseHelpersProbe.encoding("gzip, deflate").inspect
puts ResponseHelpersProbe.encoding("br, gzip;q=0.5, *;q=0.1").inspect
puts ResponseHelpersProbe.encoding("gzip;q=0").inspect
puts ResponseHelpersProbe.encoding("*;q=0").inspect
puts ResponseHelpersProbe.encoding(nil).inspect
puts ResponseHelpersProbe.etag
puts ResponseHelpersProbe.past_the_end
puts ResponseHelpersProbe.gzip_round_trip.inspect
puts ResponseHelpersProbe.snapshot.inspect
puts ResponseHelpersProbe.key
puts ResponseHelpersProbe.decoded.inspect
"#,
    expected: concat!(
        "\"gzip\"\n",
        "\"gzip\"\n",
        "\"identity\"\n",
        "nil\n",
        "\"identity\"\n",
        "W/\"dd818157edf943fbb589e25ab389d2c5\"\n",
        "true\n",
        "[[31, 139], true, true]\n",
        "[true, false, false, false, false]\n",
        "[\"rooms\",\"/rooms/1?a=\\u003cb\\u003e\\u0026c\",null,7,1.5,true,\"html\",{\"token\":\"t\",\"n\":[1,null]}]\n",
        "[\"rooms\", \"/rooms/1?a=<b>&c\", nil, 7, 1.5, true, \"html\", {\"token\" => \"t\", \"n\" => [1, nil]}]\n",
    ),
};

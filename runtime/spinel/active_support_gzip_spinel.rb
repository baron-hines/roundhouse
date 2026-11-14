# Spinel's `ActiveSupport::Gzip` (activesupport `lib/active_support/gzip.rb`), on Spinel's zlib package.
# Not Rails' `Zlib::GzipReader.wrap(StringIO.new(source), &:read)`: the package answers gunzip / gzip on Strings and
# carries no GzipReader. Loaded only where the tree names `ActiveSupport::Gzip`.
require "zlib"

module ActiveSupport
  module Gzip
    def self.decompress(source)
      Zlib.gunzip(source)
    end

    def self.compress(source, level = Zlib::DEFAULT_COMPRESSION)
      Zlib.gzip(source, level: level)
    end
  end
end

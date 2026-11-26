# frozen_string_literal: true

# `ActiveSupport::Gzip`, ported from activesupport 7.2 `lib/active_support/gzip.rb` (MIT license, Copyright (c)
# David Heinemeier Hansson): a String gzipped and back, as lucide-rails reads its icon bundle.
require "zlib"
require "stringio"

module ActiveSupport
  module Gzip
    class Stream < StringIO
      def initialize(*)
        super
        set_encoding "BINARY"
      end

      def close
        rewind
      end
    end

    def self.decompress(source)
      Zlib::GzipReader.wrap(StringIO.new(source), &:read)
    end

    def self.compress(source, level = Zlib::DEFAULT_COMPRESSION, strategy = Zlib::DEFAULT_STRATEGY)
      output = Stream.new
      gz = Zlib::GzipWriter.new(output, level, strategy)
      gz.write(source)
      gz.close
      output.string
    end
  end
end

# frozen_string_literal: true
# `Rack::BodyProxy`, ported from rack 3.2 (lib/rack/body_proxy.rb) for the trees that have no rack gem to load; the
# ruby family swaps this file for `require "rack/body_proxy"` (see `project::ruby_runtime_files`).
#
# A middleware wraps the body it hands up so its block runs once the server is done with the body (request_store's
# `RequestStore.clear!`): the server closes it, as Rack's SPEC asks, and `close` runs the block. rack's proxy forwards every other method to the body through `method_missing`; here the body is the Array
# of Strings the emitted stack answers, and `each` is what the server reads it with.
module Rack
  class BodyProxy
    def initialize(body, &block)
      @body = body
      @block = block
      @closed = false
    end

    def each(&block)
      @body.each(&block)
      self
    end

    def close
      return nil if @closed
      @closed = true
      # Not the body's own close first, as rack's does: the emitted stack's body is an Array of Strings, which has none.
      @block.call
      nil
    end

    def closed?
      @closed
    end
  end
end

# frozen_string_literal: true
# rack 3.2's `Rack::Response`, buffered, for the Spinel tree, which has no
# rack gem to load (Warden's manager tells one from the Array an app
# answers). The ruby family leaves this file out
# (`project::ruby_family_runtime_files`): rack's own class runs there.
#
# Headers are kept with lowercase names, as rack's `Rack::Headers` keeps
# them. A body is a String or an Array of Strings.
module Rack
  class Response
    attr_accessor :length, :status, :body
    attr_reader :headers

    def initialize(body = nil, status = 200, headers = {})
      @status = status.to_i
      @headers = {}
      headers.each { |key, value| @headers[key.to_s.downcase] = value }
      @body = []
      # Not 0 for no body: rack leaves the length unknown, so `finish` writes no content-length.
      @length = nil
      if body.is_a?(String)
        @body << body
        @length = body.bytesize
      elsif body.is_a?(Array)
        body.each { |part| @body << part.to_s }
      end
    end

    def redirect(target, status = 302)
      @status = status
      set_header("location", target)
    end

    def chunked?
      get_header("transfer-encoding") == "chunked"
    end

    def no_entity_body?
      (@status >= 100 && @status < 200) || @status == 204 || @status == 304
    end

    def finish
      if no_entity_body?
        delete_header("content-type")
        delete_header("content-length")
        return [@status, @headers, []]
      end
      @headers["content-length"] = @length.to_s if !@length.nil? && !chunked?
      [@status, @headers, @body]
    end

    def to_a
      finish
    end

    def each
      @body.each { |part| yield part }
    end

    def write(chunk)
      part = chunk.to_s
      # Not from 0: rack buffers an Array body on the first write, counting the parts already there.
      held = @length.nil? ? @body.sum { |each_part| each_part.bytesize } : @length.to_i
      @body << part
      @length = held + part.bytesize
      part
    end

    def close
      nil
    end

    def empty?
      @body.empty?
    end

    def has_header?(key)
      @headers.key?(key.downcase)
    end

    def get_header(key)
      @headers[key.downcase]
    end

    def set_header(key, value)
      @headers[key.downcase] = value
    end

    def delete_header(key)
      @headers.delete(key.downcase)
    end

    def [](key)
      get_header(key)
    end

    def []=(key, value)
      set_header(key, value)
    end

    def location
      get_header("location")
    end

    def location=(target)
      set_header("location", target)
    end

    def content_type
      get_header("content-type")
    end

    def content_type=(type)
      set_header("content-type", type)
    end

    def informational?
      @status >= 100 && @status < 200
    end

    def successful?
      @status >= 200 && @status < 300
    end

    def redirection?
      @status >= 300 && @status < 400
    end

    def client_error?
      @status >= 400 && @status < 500
    end

    def server_error?
      @status >= 500 && @status < 600
    end

    def ok?
      @status == 200
    end

    def not_found?
      @status == 404
    end

    def redirect?
      [301, 302, 303, 307, 308].include?(@status)
    end
  end
end

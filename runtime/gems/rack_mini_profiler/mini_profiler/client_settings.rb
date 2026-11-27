# frozen_string_literal: true
module Rack
  class MiniProfiler
    class ClientSettings
      COOKIE_NAME = "__profilin"

      # Not nil for the gem's BACKTRACE_DEFAULT: the slot stays an Integer, and the gem reads a cookie's `bt=0` as the default too.
      BACKTRACE_DEFAULT = 0
      BACKTRACE_FULL = 1
      BACKTRACE_NONE = 2

      # Not nil/true/false: the gem compares the cookie's value with the request's, and nil (no `dp`) differs from false.
      UNSET = -1
      OFF = 0
      ON = 1

      attr_accessor :disable_profiling, :backtrace_level

      def initialize(req, store, start)
        @store = store
        @start = start
        @has_cookie = req.cookies.key?(COOKIE_NAME)
        @cookie = req.cookies.fetch(COOKIE_NAME, "")
        @backtrace_level = BACKTRACE_DEFAULT
        @disable_profiling = UNSET
        @orig_disable_profiling = UNSET
        @orig_auth_tokens = [""]
        @orig_auth_tokens.pop
        @has_orig_auth_tokens = false
        @allowed_tokens = [""]
        @allowed_tokens.pop
        @has_allowed_tokens = false
        if @has_cookie
          @cookie.split(",").each do |pair|
            kv = pair.split("=")
            k = kv.length > 0 ? kv[0] : ""
            v = kv.length > 1 ? kv[1] : ""
            if k == "dp"
              @disable_profiling = v == "t" ? ON : OFF
              @orig_disable_profiling = @disable_profiling
            end
            @backtrace_level = v.to_i if k == "bt"
            if k == "a"
              @orig_auth_tokens = v.split("|")
              @has_orig_auth_tokens = true
            end
          end
        end
        @backtrace_level = BACKTRACE_DEFAULT if @backtrace_level < 0 || @backtrace_level > BACKTRACE_NONE
        @orig_backtrace_level = @backtrace_level
      end

      def handle_cookie(res, ssl)
        if MiniProfiler.config.allow_authorized? && !MiniProfiler.authorized?
          # The gem keeps the cookie on errors and short requests, so a page that never reached Rails does not end profiling.
          elapsed = Process.clock_gettime(Process::CLOCK_MONOTONIC) - @start
          discard_cookie!(res) if res.status >= 200 && res.status < 300 && elapsed > 0.1
        else
          write!(res, ssl)
        end
        nil
      end

      def write!(res, ssl)
        tokens_changed = false
        if MiniProfiler.authorized? && MiniProfiler.config.allow_authorized?
          load_allowed_tokens
          tokens_changed = !@has_orig_auth_tokens || (@allowed_tokens - @orig_auth_tokens).length > 0
        end
        if @orig_disable_profiling != @disable_profiling ||
           @orig_backtrace_level != @backtrace_level ||
           !@has_cookie ||
           tokens_changed
          settings = ["p=t"]
          settings << "dp=t" if @disable_profiling == ON
          settings << "bt=" + @backtrace_level.to_s if @backtrace_level != BACKTRACE_DEFAULT
          settings << "a=" + @allowed_tokens.join("|") if @has_allowed_tokens && MiniProfiler.authorized?
          opts = Tep.str_hash
          opts["path"] = MiniProfiler.config.cookie_path
          opts["secure"] = +"" if ssl
          opts["httponly"] = +""
          opts["samesite"] = "lax"
          res.set_cookie(COOKIE_NAME, settings.join(","), opts)
        end
        nil
      end

      def discard_cookie!(res)
        return nil unless @has_cookie
        opts = Tep.str_hash
        opts["path"] = MiniProfiler.config.cookie_path
        opts["max-age"] = "0"
        opts["expires"] = "Thu, 01 Jan 1970 00:00:00 GMT"
        res.set_cookie(COOKIE_NAME, "", opts)
        nil
      end

      def has_valid_cookie?
        return false unless @has_cookie
        return true unless MiniProfiler.config.allow_authorized?
        load_allowed_tokens
        @has_orig_auth_tokens && (@allowed_tokens & @orig_auth_tokens).length > 0
      end

      def disable_profiling?
        @disable_profiling == ON
      end

      def backtrace_full?
        @backtrace_level == BACKTRACE_FULL
      end

      def backtrace_default?
        @backtrace_level == BACKTRACE_DEFAULT
      end

      def backtrace_none?
        @backtrace_level == BACKTRACE_NONE
      end

      def load_allowed_tokens
        return nil if @has_allowed_tokens
        @allowed_tokens = @store.allowed_tokens
        @has_allowed_tokens = true
        nil
      end
    end
  end
end

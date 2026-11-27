# frozen_string_literal: true
module Rack
  class MiniProfiler
    class Config
      attr_reader :authorization_mode
      attr_accessor :auto_inject, :base_url_path, :cookie_path,
        :disable_caching, :enabled, :enable_advanced_debugging_tools, :skip_paths,
        :storage_path, :collapse_results, :max_traces_to_show, :position,
        :show_children, :show_controls, :show_trivial, :show_total_sql_count,
        :start_hidden, :toggle_shortcut, :html_container,
        :enable_hotwire_turbo_drive_support, :profile_parameter,
        :snapshot_hidden_custom_fields, :pre_authorized

      # The gem's own setter: its deprecated `:whitelist` is `:allow_authorized`, which a plain writer would leave unrecognized and so profile every request.
      def authorization_mode=(mode)
        mode = :allow_authorized if mode == :whitelist
        @authorization_mode = mode
      end

      def initialize
        @auto_inject = true
        @base_url_path = "/mini-profiler-resources/"
        @cookie_path = "/"
        @disable_caching = true
        @authorization_mode = :allow_all
        @enabled = true
        @enable_advanced_debugging_tools = false
        @skip_paths = [""]
        @skip_paths.pop
        @storage_path = "tmp/miniprofiler"
        @collapse_results = true
        @max_traces_to_show = 20
        @show_children = false
        @show_controls = false
        @show_trivial = false
        @show_total_sql_count = false
        @start_hidden = false
        @toggle_shortcut = "alt+p"
        @html_container = "body"
        @position = "top-left"
        @snapshot_hidden_custom_fields = [""]
        @snapshot_hidden_custom_fields.pop
        @enable_hotwire_turbo_drive_support = false
        @profile_parameter = "pp"
        # Not a `pre_authorize_cb` lambda: the one the Railtie installs only reads Rails.env, so its answer is fixed at boot.
        @pre_authorized = true
      end

      # MiniProfilerRails.initialize! for this tree: the gem's Railtie runs before the app's initializers, which may override any of it.
      def apply_railtie(env)
        @pre_authorized = !env.test?
        @authorization_mode = :allow_authorized unless env.development? || env.test?
        @storage_path = "tmp/miniprofiler"
        # Not `= env.development?` as the Railtie sets it: those tools read CRuby's heap and ObjectSpace, which a Spinel binary has no equivalent of.
        @enable_advanced_debugging_tools = false
        nil
      end

      def vertical_position
        @position.include?("bottom") ? "bottom" : "top"
      end

      def horizontal_position
        @position.include?("right") ? "right" : "left"
      end

      # Rack::MiniProfiler#initialize appends the slash; the app's initializer may have set the path without one.
      def resources_path
        @base_url_path.end_with?("/") ? @base_url_path : @base_url_path + "/"
      end

      def allow_authorized?
        @authorization_mode == :allow_authorized
      end
    end
  end
end

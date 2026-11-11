# frozen_string_literal: true
# ActionDispatch::Routing::RouteSet: the routes are drawn when the app is
# compiled, so what is left of Rails' route set is `finalize!`, which Rails
# runs once after the middleware stack is built (the Finisher's
# set_routes_reloader_hook) and a gem prepends to (Devise's
# `configure_warden!`, which reads the warden config the stack's block set).
module ActionDispatch
  module Routing
    # What `include Rails.application.routes.mounted_helpers` gives: `main_app`, the app's own routes. The app
    # mounts no engine of its own routes here, so it is the only proxy.
    module MountedHelpers
      def main_app
        RouteHelpers
      end
    end

    class RouteSet
      def finalize!
        nil
      end

      # The app's route set, finalized once: what the stack calls after it is built.
      def self.finalize_app!
        return nil if @finalized
        @app = new
        @app.finalize!
        # Not before `finalize!` returns: until then the routes are not finalized, and a caller that raised there tries again.
        @finalized = true
        nil
      end
    end
  end
end

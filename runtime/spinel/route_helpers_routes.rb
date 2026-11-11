# frozen_string_literal: true
# Rails' `main_app.routes.url_helpers`, which a gem asks of the router it holds (Devise's FailureApp: `defined?(context.routes)
# && context.routes.url_helpers.respond_to?(:root_path)`). Ruby family and Spinel only: `RouteHelpers` is the program's
# generated module, which the strict targets do not reopen.
module ActionDispatch
  module Routing
    class RouteSet
      def url_helpers
        RouteHelpers
      end
    end
  end
end

module RouteHelpers
  def self.routes
    ActionDispatch::Routing::RouteSet.new
  end
end

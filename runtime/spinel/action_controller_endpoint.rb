# frozen_string_literal: true
# Rails' `Controller.action(name)`: the Rack endpoint that runs one action of a new controller per request
# (`ActionController::Metal.action`, which Base inherits). Devise's FailureApp answers Warden through
# `action(:respond)`, and its recall through the action a controller's name and an action's name give.
#
# The request is the one the routes were given (`GemMiddleware.endpoint`): a middleware calls an endpoint while
# the request is still its own, and Warden's failure app is called with the same env the routes saw.
module ActionController
  class Base
    def self.action(name)
      ActionController::Endpoint.new(self, name.to_sym)
    end

    # Metal's `response=` of a Rack response (Devise's recall: `self.response = recall_app(…).call(env)`), which the
    # controller then answers in place of what it rendered.
    def response=(rack)
      @rack_response = rack
    end

    def rack_response
      @rack_response
    end
  end

  class Endpoint
    def initialize(controller_class, action)
      @controller_class = controller_class
      @action = action
    end

    def call(env)
      GemMiddleware.endpoint(@controller_class.new, @action, env)
    end
  end
end

# frozen_string_literal: true
# The Rack middleware the app's gems put on Rails' stack from their
# Railties (`App::gem_middlewares`), between Rails' RequestId and the
# routes. No gem of this app inserts one; the project writes this file
# over when one does (`project::spinel_gem_middleware`).
module GemMiddleware
  def self.dispatch(req, res, request_obj)
    Main.dispatch_request(req, res, request_obj)
    nil
  end

  # No middleware of a gem calls an endpoint where none is on the stack.
  def self.endpoint(controller, action, env)
    raise NotImplementedError, "no middleware of a gem is on the stack to call an endpoint"
  end

  # The test harness's request through the stack (Rails' integration tests run it): with no gem
  # middleware, the block is the whole stack.
  def self.around(env)
    yield(env)
  end
end

# frozen_string_literal: true
# The Rack middleware the app's gems put on Rails' stack from their
# Railties (`App::gem_middlewares`), around the app `Main.run_rack`
# answers. No gem of this app inserts one; the project writes this file
# over when one does.
module GemMiddleware
  def self.wrap(app)
    app
  end

  # The test harness's request through the stack (Rails' integration tests run it), the block at its bottom.
  def self.around(env, &inner)
    wrap(inner).call(env)
  end

  # `Controller.action(name)`'s endpoint (runtime/action_controller_endpoint.rb), over the env it is called with.
  def self.endpoint(controller, action, env)
    Main.run_endpoint(controller, action, env)
  end
end

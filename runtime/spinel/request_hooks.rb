# frozen_string_literal: true
# Not a Rack middleware stack: the Spinel server has none, so a gem package whose Railtie inserts a middleware registers a Hook here.
module RequestHooks
  class Hook
    def request_begin(req, res)
      false
    end

    def request_end(req, res)
      nil
    end
  end

  # Not `[]`: spinel types the Array from its first element, and an empty literal cannot hold a subclass later.
  HOOKS = [Hook.new]
  HOOKS.pop

  def self.register(hook)
    HOOKS.push(hook)
    nil
  end

  def self.request_begin(req, res)
    i = 0
    answered = false
    while i < HOOKS.length && !answered
      answered = HOOKS[i].request_begin(req, res)
      i += 1
    end
    # Not a local count handed back: request_end runs from Main.dispatch, which only knows whether the request was answered.
    Thread.current[:request_hooks_begun] = i
    answered
  end

  def self.request_end(req, res)
    begun = Thread.current[:request_hooks_begun]
    i = begun.nil? ? 0 : begun
    while i > 0
      i -= 1
      HOOKS[i].request_end(req, res)
    end
    nil
  end
end

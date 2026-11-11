# frozen_string_literal: true
# ActionController::Metal and the modules a Metal subclass mixes back in
# (devise's FailureApp: `class FailureApp < ActionController::Metal;
# include ActionController::UrlFor, ActionController::Redirecting,
# AbstractController::Callbacks`). Rails builds Base from Metal and these
# modules; here Base already holds what they give, so Metal is Base under
# Metal's name, as API is (api.rb), and the modules are empty, though Base
# includes them as Rails' does.
#
# ActionView::Rendering is the one a caller asks about by name
# (responders' `controller.class.include? ActionView::Rendering`): Base
# takes it through ActionView::Layouts, and API and Metal, built without
# views, do not.
module AbstractController
  module Callbacks
  end
end

module ActionView
  module Rendering
  end
end

module ActionController
  module UrlFor
  end

  module Redirecting
  end

  class Base
    include AbstractController::Callbacks
    include UrlFor
    include Redirecting
    include ActionView::Rendering

    def logger
      Rails.logger
    end

    # ActionView::Rendering's: the format respond_with negotiated is the one the response's templates are looked up in.
    def _process_format(format)
      negotiated = format.to_sym
      @request_format = negotiated unless negotiated.nil?
      nil
    end
  end

  class API < Base
    # Not `super`: Spinel has no Module#include? to send it to.
    def self.include?(mod)
      mod != ActionView::Rendering && ancestors.include?(mod)
    end
  end

  class Metal < Base
    # Built without views, as API is.
    def self.include?(mod)
      mod != ActionView::Rendering && ancestors.include?(mod)
    end

    # What Rails' AbstractController::Callbacks registers, so a Metal subclass's body loads (Devise's FailureApp,
    # which the boot names). Not kept here: `ingest::metal_actions` writes a single around block into the class's own
    # `process_action`.
    def self.around_action(*names, &block)
      nil
    end

    def self.before_action(*names, &block)
      nil
    end

    def self.after_action(*names, &block)
      nil
    end
  end
end

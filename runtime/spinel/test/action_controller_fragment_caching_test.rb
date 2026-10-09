require_relative "test_helper"
require_relative "../runtime/action_controller_fragment_caching"

class FragmentCachingDetachedControllerTest < TestBase
  def test_detached_fragments_do_not_use_the_shared_cache
    ActionController::Current.controller = nil
    controller = ActionView::ViewHelpers.fragment_controller

    assert !controller.perform_caching
  end
end

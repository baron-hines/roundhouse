require_relative "test_helper"
require_relative "../runtime/action_controller_fragment_caching"

class FragmentCachingDetachedControllerTest < TestBase
  def test_detached_fragments_do_not_use_the_shared_cache
    ActionController::Current.controller = nil
    controller = ActionView::ViewHelpers.fragment_controller

    assert !controller.perform_caching
  end

  class NilParentController < ActionController::Base
  end

  class NilChildController < NilParentController
  end

  class AssignedParentController < ActionController::Base
  end

  class AssignedChildController < AssignedParentController
  end

  def test_explicit_nil_settings_override_inherited_values
    NilParentController.perform_caching = nil
    assert NilParentController.perform_caching.nil?
    assert NilChildController.perform_caching.nil?

    NilParentController.cache_store = nil
    assert NilParentController.cache_store.nil?
    assert NilChildController.cache_store.nil?
  end

  def test_assigned_settings_are_inherited_and_child_settings_are_local
    AssignedParentController.perform_caching = false
    assert !AssignedParentController.perform_caching
    assert !AssignedChildController.perform_caching

    AssignedChildController.perform_caching = true
    assert AssignedChildController.perform_caching
    assert !AssignedParentController.perform_caching

    store = ActiveSupport::Cache::MemoryStore.new
    AssignedParentController.cache_store = store
    assert AssignedParentController.cache_store.equal?(store)
    assert AssignedChildController.cache_store.equal?(store)
  end
end

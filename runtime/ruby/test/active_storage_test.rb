# frozen_string_literal: true
require_relative "test_helper"
require "active_storage"

# `Document has_one_attached :file` reads `document.file` as an
# `ActiveStorage::Attached` proxy. Rails' `Attached::One` delegates
# missing methods to its attachment with `allow_nil: true`, so
# `document.file.download` is the blob's bytes, and nil when nothing is
# attached.
class ActiveStorageAttachedTest < Minitest::Test
  class FakeService < ActiveStorage::Service
    def download(key)
      "bytes-of-" + key
    end
  end

  def setup
    @service = FakeService.new
    service = @service
    @original = ActiveStorage::Blob.method(:service)
    ActiveStorage::Blob.define_singleton_method(:service) { service }
  end

  def teardown
    original = @original
    ActiveStorage::Blob.define_singleton_method(:service) { |*args| original.call(*args) }
  end

  def proxy
    ActiveStorage::Attached.new("Document", 1, "file", [])
  end

  def test_download_returns_the_blobs_bytes_when_attached
    blob = ActiveStorage::Blob.new(7, "abc123", "a.txt", "text/plain", 5,
                                   ActiveStorage::BlobMetadata.parse(""))
    attached = proxy
    attached._preload_row(3, blob)
    assert_equal "bytes-of-abc123", attached.download
  end

  def test_download_is_nil_when_nothing_is_attached
    attached = proxy
    attached._preload_row(0, nil)
    assert_nil attached.download
  end
end

# frozen_string_literal: true
module ActiveSupport
  # Shared implementation used by the typed Hash#deep_merge call lowering.
  # Recursively combine only Hash/Hash value pairs; all other right-hand
  # values replace the left, as Rails 8.1.4 does.
  def self.deep_merge(left, other_hash)
    merged = left.dup
    other_hash.each do |key, other_value|
      if left.key?(key) && left[key].is_a?(Hash) && other_value.is_a?(Hash)
        current_value = left[key]
        merged[key] = ActiveSupport.deep_merge(current_value, other_value)
      else
        merged[key] = other_value
      end
    end
    merged
  end

  # Rails' deep_merge!: the receiver's own entries are written in place, and a Hash/Hash pair becomes the merge of the
  # two, a new Hash, as Rails 8.1.4's `this_val.deep_merge(other_val)` gives it. The receiver is the answer.
  def self.deep_merge!(left, other_hash)
    other_hash.each do |key, other_value|
      if left.key?(key) && left[key].is_a?(Hash) && other_value.is_a?(Hash)
        current_value = left[key]
        left[key] = ActiveSupport.deep_merge(current_value, other_value)
      else
        left[key] = other_value
      end
    end
    left
  end
end
